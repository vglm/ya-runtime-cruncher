use actix::prelude::*;
use chrono::Utc;
use clap::Parser;
use futures::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc, mpsc::Receiver, mpsc::Sender, Mutex};
use ya_client_model::activity::activity_state::*;
use ya_client_model::activity::{ActivityUsage, CommandResult, ExeScriptCommandResult};
use ya_client_model::activity::{CommandOutput, ExeScriptCommand};
use ya_core_model::activity;
use ya_core_model::activity::exeunit::bus_id;
use ya_core_model::activity::RpcMessageError;
use ya_service_bus::typed::{self as gsb, Endpoint};
use ya_transfer::transfer::{Shutdown, TransferService, TransferServiceContext};

use crate::agreement::AgreementDesc;
use crate::cli::*;
use crate::logger::*;
use crate::process::dummy::{Config, Dummy};
use crate::signal::SignalMonitor;

mod agreement;
mod cli;
mod logger;
mod offer_template;
mod process;
mod signal;

pub type Signal = &'static str;

async fn send_state(ctx: &ExeUnitContext, new_state: ActivityState) -> anyhow::Result<()> {
    Ok(gsb::service(ctx.report_url.clone())
        .call(activity::local::SetState::new(
            ctx.activity_id.clone(),
            new_state,
            None,
        ))
        .await??)
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

#[allow(unused)]
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
        log::error!("Cruncher Runtime panic: {e}");
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
    let runtime_config = Dummy::parse_config(&cli.runtime_config)?;

    match cli.runtime.to_lowercase().as_str() {
        "dummy" => run(cli, signal_receiver, runtime_config).await,
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
struct ExeUnitContext {
    pub activity_id: String,
    pub report_url: String,

    pub transfers: Addr<TransferService>,
    pub batches: Rc<RefCell<HashMap<String, Vec<ExeScriptCommandResult>>>>,
}

async fn prepare_script_future(
    ctx: ExeUnitContext,
    exec: activity::Exec,
    current_usage: Arc<Mutex<Vec<f64>>>,
    tera_hash_pos: usize,
) -> Result<String, RpcMessageError> {
    let mut result = Vec::new();
    for exe in &exec.exe_script {
        match exe {
            ExeScriptCommand::Deploy { .. } => {}
            ExeScriptCommand::Start { args, .. } => {
                log::debug!("Raw Start cmd args: {args:?} [ignored]");

                set_usage_msg(
                    &gsb::service(ctx.report_url.clone()),
                    &ctx.activity_id,
                    current_usage.lock().await.clone(),
                )
                .await;

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
                //mark capture as unused
                let _capture = capture;
                log::debug!("Parameter capture ignored");

                let command = entry_point;
                log::info!("Receive command {command} with args {}", args.join(" "));

                if command == "set_hash" {
                    {
                        if let Some(tera_hash) = args.first() {
                            if let Ok(tera_hash) = tera_hash.parse::<f64>() {
                                current_usage.lock().await[tera_hash_pos] = tera_hash;
                            }
                        }
                    }
                    set_usage_msg(
                        &gsb::service(ctx.report_url.clone()),
                        &ctx.activity_id,
                        current_usage.lock().await.clone(),
                    )
                    .await;
                } else {
                    log::error!("Invalid command for cruncher runtime: {:?}", command);
                    return Err(RpcMessageError::Activity(format!(
                        "invalid command for cruncher runtime: {:?}",
                        command
                    )));
                }

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

async fn run(
    cli: Cli,
    mut signal_receiver: Receiver<Signal>,
    runtime_config: Config,
) -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    log::info!("Runtime config: {runtime_config:?}");

    let (exe_unit_url, report_url, activity_id, args) = match &cli.command {
        Command::ServiceBus {
            service_id,
            report_url,
            args,
            ..
        } => (bus_id(service_id), report_url, service_id, args),
        Command::OfferTemplate => {
            let offer_template = Dummy::offer_template(&runtime_config)?;
            let offer_template = serde_json::to_string_pretty(&offer_template)?;
            io::stdout().write_all(offer_template.as_bytes())?;
            return Ok(());
        }
        Command::Test => {
            return Ok(());
        }
    };

    let agreement_path = args.agreement.clone();

    let agreement = AgreementDesc::load(agreement_path)?;

    if agreement.counters.len() != 2 {
        log::error!("Invalid agreement. Expected 2 usage counters");
        anyhow::bail!("Invalid agreement. Expected 2 usage counters");
    }
    let mut duration_sec_pos = agreement.counters.len();
    let mut tera_hash_pos = agreement.counters.len();
    for counter in agreement.counters.iter().enumerate() {
        if counter.1 == "golem.usage.duration_sec" {
            duration_sec_pos = counter.0;
        } else if counter.1 == "golem.usage.tera-hash" {
            tera_hash_pos = counter.0;
        }
    }
    if duration_sec_pos < agreement.counters.len() && tera_hash_pos < agreement.counters.len() {
        log::info!(
            "Found usage counters: tera-hash={}, duration_sec={}",
            tera_hash_pos,
            duration_sec_pos
        );
    } else {
        log::error!("Invalid agreement. Missing usage counters");
        anyhow::bail!("Invalid agreement. Missing usage counters");
    }

    let ctx = ExeUnitContext {
        activity_id: activity_id.clone(),
        report_url: report_url.clone(),
        transfers: TransferService::new(TransferServiceContext {
            work_dir: args.work_dir.clone(),
            cache_dir: args.cache_dir.clone(),
            ..TransferServiceContext::default()
        })
        .start(),
        batches: Rc::new(RefCell::new(Default::default())),
    };

    let current_usage = Arc::new(Mutex::new(vec![0.0, 0.0]));

    {
        let batch = ctx.batches.clone();
        let batch_results = batch.clone();

        let ctx = ctx.clone();
        gsb::bind(&exe_unit_url, move |exec: activity::Exec| {
            let current_usage = current_usage.clone();
            let exec = exec.clone();
            let batch = batch.clone();
            let batch_id = exec.batch_id.clone();
            let batch_id_ = exec.batch_id.clone();

            {
                let _ = ctx
                    .batches
                    .borrow_mut()
                    .insert(exec.batch_id.clone(), vec![]);
            }
            let ctx = ctx.clone();
            let script_future =
                prepare_script_future(ctx.clone(), exec, current_usage.clone(), tera_hash_pos)
                    .map_err(move |e| {
                        log::error!("ExeScript failure: {e:?}");
                        let mut bind_batch = batch.borrow_mut();
                        let result = bind_batch.entry(batch_id_).or_default();

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
    //note that we are here immediately after the bind to gsb
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Initialized, None)),
    )
    .await?;

    let signal = signal_receiver.recv().await;

    if let Some(signal) = signal {
        log::debug!("Received signal {signal}. Stopping runtime");
    }

    log::info!("Finished waiting for activity loop.");
    send_state(
        &ctx,
        ActivityState::from(StatePair(State::Terminated, None)),
    )
    .await?;

    log::info!("Activity state set to terminated.");
    Ok(())
}
