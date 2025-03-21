FROM ubuntu

# Install dependencies
RUN apt-get update && apt-get install -y curl

RUN curl -sSL https://github.com/golemfactory/ya-service-bus/releases/download/v0.6.3/ya-sb-router-linux-v0.6.3.tar.gz -o package.tar.gz && \
    tar -xzf package.tar.gz && \
    rm package.tar.gz && mv ya-sb-router-linux-v0.6.3/ya-sb-router /usr/bin/ya-sb-router && rm -rf ya-sb-router-linux-v0.6.3

