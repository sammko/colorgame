FROM rust:1.62-buster AS build

# Create empty project so we can build only dependencies
RUN cargo new --bin colorgame
WORKDIR /colorgame

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release
RUN rm src/*.rs


# Copy the source code
COPY ./sqlx-data.json ./sqlx-data.json
COPY ./src ./src
COPY ./sql ./sql
COPY ./migrations ./migrations

# Build for release.
RUN rm ./target/release/deps/colorgame*
RUN cargo build --release

# The final base image
FROM debian:buster-slim

# Copy from the previous build
COPY --from=build /colorgame/target/release/colorgame /usr/local/bin/colorgame

COPY ./config.json /config.json

EXPOSE 8000

CMD ["/usr/local/bin/colorgame"]
