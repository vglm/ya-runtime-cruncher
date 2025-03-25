FROM rust AS builder
RUN apt update && apt install -y cmake
RUN git clone https://github.com/golemfactory/ya-service-bus.git
RUN cd ya-service-bus && cargo build --bin ya-sb-router --features ya-sb-router/bin --release --package ya-sb-router

FROM ubuntu
COPY --from=builder /ya-service-bus/target/release/ya-sb-router /usr/local/bin/ya-sb-router