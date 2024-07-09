FROM rust:latest AS builder
WORKDIR /makedot

RUN mkdir src && echo "fn main() {}" > src/main.rs 
COPY ./Cargo.lock ./
COPY ./Cargo.toml ./
RUN cargo fetch 
RUN cargo build --release 
RUN rm -r src/
COPY ./src ./src
RUN cargo build --release

FROM scratch
COPY --from=builder /makedot/target/release/makedot /makedot