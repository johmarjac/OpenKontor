# OpenKontor server as a container. Build from the project folder:
#   docker build -f deploy/Dockerfile -t openkontor-server .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --no-default-features --bin openkontor-server

FROM debian:bookworm-slim
# Text recognition for scanned receipts and photos.
RUN apt-get update && apt-get install -y --no-install-recommends \
        tesseract-ocr tesseract-ocr-deu poppler-utils ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --home /data --create-home openkontor
COPY --from=build /src/target/release/openkontor-server /usr/local/bin/openkontor-server
USER openkontor
EXPOSE 8470
ENTRYPOINT ["openkontor-server", "--data", "/data"]
