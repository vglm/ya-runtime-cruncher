use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use actix::prelude::*;
use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use futures::prelude::*;
use process::Runtime;
use tokio::select;
use tokio::sync::{mpsc, mpsc::Receiver, mpsc::Sender};
use tokio::time::sleep;
use ya_client_model::activity::activity_state::*;
use ya_client_model::activity::{ActivityUsage, CommandResult, ExeScriptCommandResult};
use ya_client_model::activity::{CommandOutput, ExeScriptCommand};
use ya_core_model::activity;
use ya_core_model::activity::RpcMessageError;
use ya_counters::error::CounterError;
use ya_counters::message::GetCounters;
use ya_counters::service::{CountersService, CountersServiceBuilder};
use ya_counters::TimeCounter;
use ya_gsb_http_proxy::gsb_to_http::GsbToHttpProxy;
use ya_service_bus::typed::{self as gsb, Endpoint};
use ya_transfer::transfer::{Shutdown, TransferService, TransferServiceContext};

use crate::agreement::AgreementDesc;
use crate::cli::*;
use crate::logger::*;
use crate::process::ProcessController;
use crate::signal::SignalMonitor;

mod agreement;
mod cli;
mod logger;
mod offer_template;
mod process;
mod signal;

pub type Signal = &'static str;

async fn send_state<T: process::Runtime>(
    ctx: &ExeUnitContext<T>,
    new_state: ActivityState,
) -> anyhow::Result<()> {
    Ok(gsb::service(ctx.report_url.clone())
        .call(activity::local::SetState::new(
            ctx.activity_id.clone(),
            new_state,
            None,
        ))
        .await??)
}

async fn activity_loop<T: process::Runtime + Clone + Unpin + 'static>(
    report_url: &str,
    activity_id: &str,
    process: ProcessController<T>,
) -> anyhow::Result<()> {
    let report_service = gsb::service(report_url);

    while let Some(()) = process.report() {
        select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            status = process.clone() => {
                set_terminate_state_msg(&report_service, activity_id, Some("process exit".to_string()), Some(format!("status: {:?}", status))).await;
                log::error!("process exit: {:?}", status);
                anyhow::bail!("Runtime exited");
            }

        }
    }
    Ok(())
}

async fn set_usage_msg(report_service: &Endpoint, activity_id: &str, current_usage: Vec<f64>) {
    let timestamp = Utc::now().timestamp();
    match report_service
        .call(activity::local::SetUsage {
            activity_id: activity_id.into(),
            usage: ActivityUsage {
                current_usage: Some(current_usage),
                timestamp,
            },
            timeout: None,
        })
        .await
    {
        Ok(Ok(())) => log::trace!("Successfully sent activity usage message"),
        Ok(Err(rpc_message_error)) => {
            log::error!("rpcMessageError : {:?}", rpc_message_error)
        }
        Err(err) => log::error!("other error : {:?}", err),
    }
}

async fn set_terminate_state_msg(
    report_service: &Endpoint,
    activity_id: &str,
    reason: Option<String>,
    error_message: Option<String>,
) {
    if let Err(err) = report_service
        .call(activity::local::SetState {
            activity_id: activity_id.into(),
            state: ActivityState {
                state: StatePair::from(State::Terminated),
                reason,
                error_message,
            },
            timeout: None,
            credentials: None,
        })
        .await
    {
        log::error!("Failed to send state. Err {err}");
    }
}

#[actix_rt::main]
async fn main() {
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |e| {
        log::error!("AI Runtime panic: {e}");
        panic_hook(e)
    }));

    if let Err(error) = start_file_logger() {
        start_logger().expect("Failed to start logging");
        log::warn!("Using fallback logging due to an error: {:?}", error);
    };

    std::process::exit(match try_main().await {
        Ok(_) => 0,
        Err(error) => {
            log::error!("{}", error);
            1
        }
    })
}

async fn try_main() -> anyhow::Result<()> {
    log::debug!("Raw CLI args: {:?}", std::env::args_os());
    let cli = Cli::try_parse()?;

    let (signal_sender, signal_receiver) = mpsc::channel::<Signal>(1);

    tokio::task::spawn_local(async move {
        handle_signals(signal_sender)
            .await
            .inspect_err(|e| log::error!("Error waiting for signal: {e}"))
            .ok();
    });

    handle_cli(cli, signal_receiver).await
}

async fn handle_cli(cli: Cli, signal_receiver: Receiver<Signal>) -> anyhow::Result<()> {
    match cli.runtime.to_lowercase().as_str() {
        "dummy" => run::<process::dummy::Dummy>(cli, signal_receiver).await,
        "automatic" => run::<process::automatic::Automatic>(cli, signal_receiver).await,
        _ => {
            let err = anyhow::format_err!("Unsupported framework {}", cli.runtime);
            log::error!("{}", err);
            anyhow::bail!(err)
        }
    }
}

async fn handle_signals(signal_receiver: Sender<Signal>) -> anyhow::Result<()> {
    let signal = SignalMonitor::default().recv().await?;
    log::info!("{} received, Shutting down runtime...", signal);
    Ok(signal_receiver.send(signal).await?)
}

#[derive(Clone)]
struct ExeUnitContext<T: Runtime + 'static> {
    pub activity_id: String,
    pub report_url: String,

    pub agreement: AgreementDesc,
    pub transfers: Addr<TransferService>,
    pub process_controller: ProcessController<T>,

    pub batches: Rc<RefCell<HashMap<String, Vec<ExeScriptCommandResult>>>>,

    pub model_path: Option<PathBuf>,
}

async fn run<RUNTIME: process::Runtime + Clone + Unpin + 'static>(
    cli: Cli,
    mut signal_receiver: Receiver<Signal>,
) -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let runtime_config = RUNTIME::parse_config(&cli.runtime_config)?;
    log::info!("Runtime config: {runtime_config:?}");

    let (exe_unit_url, report_url, activity_id, args) = match &cli.command {
        Command::ServiceBus {
            service_id,
            report_url,
            args,
            ..
        } => (
            ya_core_model::activity::exeunit::bus_id(service_id),
            report_url,
            service_id,
            args,
        ),
        Command::OfferTemplate => {
            let offer_template = RUNTIME::offer_template(&runtime_config)?;
            let offer_template = serde_json::to_string_pretty(&offer_template)?;
            io::stdout().write_all(offer_template.as_bytes())?;
            return Ok(());
        }
        Command::Test => return RUNTIME::test(&runtime_config),
    };

    let runtime_config = Box::pin(runtime_config);

    let agreement_path = args.agreement.clone();

    let agreement = AgreementDesc::load(agreement_path)?;

    //let mut gsb_proxy = GsbToHttpProxy::new("http://localhost:7861/".into());

    let ctx = ExeUnitContext {
        activity_id: activity_id.clone(),
        report_url: report_url.clone(),
        agreement,
        transfers: TransferService::new(TransferServiceContext {
            work_dir: args.work_dir.clone(),
            cache_dir: args.cache_dir.clone(),
            ..TransferServiceContext::default()
        })
        .start(),
        process_controller: process::ProcessController::<RUNTIME>::new(),
        batches: Rc::new(RefCell::new(Default::default())),
        model_path: None,
    };

    let activity_pinger = activity_loop(report_url, activity_id, ctx.process_controller.clone());

    {
        let batch = ctx.batches.clone();
        let batch_results = batch.clone();

        let ctx = ctx.clone();
        gsb::bind(&exe_unit_url, move |exec: activity::Exec| {
            let exec = exec.clone();
            let batch = batch.clone();
            let batch_id = exec.batch_id.clone();
            let batch_id_ = exec.batch_id.clone();
            let runtime_config = runtime_config.clone();

            {
                let _ = ctx
                    .batches
                    .borrow_mut()
                    .insert(exec.batch_id.clone(), vec![]);
            }
            let mut ctx = ctx.clone();
            let script_future = async move {
                let mut result = Vec::new();
                for exe in &exec.exe_script {
                    match exe {
                        ExeScriptCommand::Deploy { .. } => {}
                        ExeScriptCommand::Start { args, .. } => {
                            log::debug!("Raw Start cmd args: {args:?} [ignored]");

                            set_usage_msg(&ctx, &exec.activity_id, vec![0.0, 0.0, 0.0])
                                .await
                                .map_err(|e| RpcMessageError::Service(e.to_string()))?;
                            send_state(
                                &ctx,
                                ActivityState::from(StatePair(State::Ready, Some(State::Ready))),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;

                            log::info!("Got start command, changing state of exe unit to ready",);
                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: None,
                                stderr: None,
                                message: None,
                                is_batch_finished: true,
                                event_date: Utc::now(),
                            })
                        }
                        ExeScriptCommand::Terminate { .. } => {
                            log::info!("Raw Terminate command. Stopping runtime",);
                            if let Err(err) = ctx.process_controller.stop().await {
                                log::error!("Failed to terminate process. Err {err}");
                            }
                            ctx.transfers.send(Shutdown {}).await.ok();
                            send_state(
                                &ctx,
                                ActivityState::from(StatePair(State::Terminated, None)),
                            )
                            .await
                            .map_err(|e| RpcMessageError::Service(e.to_string()))?;
                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: None,
                                stderr: None,
                                message: None,
                                is_batch_finished: false,
                                event_date: Utc::now(),
                            });
                        }
                        ExeScriptCommand::Run {
                            entry_point,
                            args,
                            capture,
                        } => {
                            let command = entry_point;
                            log::info!("Receive command {command} with args {}", args.join(" "));
                            //let capture = capture;
                            log::debug!("Parameter capture ignored");

                            result.push(ExeScriptCommandResult {
                                index: result.len() as u32,
                                result: CommandResult::Ok,
                                stdout: Some(CommandOutput::Str("".to_string())),
                                stderr: Some(CommandOutput::Str("".to_string())),
                                message: Some("Ok".to_string()),
                                is_batch_finished: true,
                                event_date: Utc::now(),
                            });
                        }
                        cmd => {
                            log::error!("invalid command for ai runtime: {:?}", cmd);
                            return Err(RpcMessageError::Activity(format!(
                                "invalid command for ai runtime: {:?}",
                                cmd
                            )));
                        }
                    }
                }
                log::info!(
                    "got exec {}, batch_id={}, script={:?}",
                    exec.activity_id,
                    exec.batch_id,
                    exec.exe_script
                );

                {
                    let _ = ctx
                        .batches
                        .borrow_mut()
                        .insert(exec.batch_id.clone(), result);
                }

                Ok(exec.batch_id)
            }
            .map_err(move |e| {
                log::error!("ExeScript failure: {e:?}");
                let mut bind_batch = batch.borrow_mut();
                let result = bind_batch.entry(batch_id_).or_insert(vec![]);

                let index = result.len() as u32;
                result.push(ExeScriptCommandResult {
                    index,
                    result: CommandResult::Error,
                    stdout: None,
                    stderr: None,
                    message: Some(e.to_string()),
                    is_batch_finished: true,
                    event_date: Utc::now(),
                });
            });
            tokio::task::spawn_local(script_future);
            future::ok(batch_id)
        });

        gsb::bind(&exe_unit_url, move |exec: activity::GetExecBatchResults| {
            if let Some(result) = batch_results.borrow().get(&exec.batch_id) {
                future::ok(result.clone())
            } else {
                future::err(RpcMessageError::NotFound(format!(
                    "Batch id={}",
                    exec.batch_id
                )))
            }
        });
    };
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Initialized, None)),
    )
    .await?;

    select! {
        res = activity_pinger => { res }
        signal = signal_receiver.recv() => {
            if let Some(signal) = signal {
                log::debug!("Received signal {signal}. Stopping runtime");

                ctx.process_controller.stop().await
                    .context("Stopping runtime error")?;
            }
            Ok(())
        },
    }
    .context("Activity loop error")?;

    log::info!("Finished waiting for activity loop.");
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Terminated, None)),
    )
    .await?;

    log::info!("Activity state set to terminated.");
    Ok(())
}
