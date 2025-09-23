# Reference: https://docs.docker.com/reference/dockerfile/
FROM mcr.microsoft.com/windows:10.0.19042.1889 AS builder

RUN curl https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe -o rustup-init.exe
RUN rustup-init.exe -y --default-toolchain 1.89.0
RUN rustup target add x86_64-pc-windows-msvc

# Delay COPY operation as much as possible to avoid cache invalidation: https://stackoverflow.com/a/48553814
COPY . C:/app

WORKDIR C:/app
RUN cargo build --release -p wm-server

FROM mcr.microsoft.com/windows:10.0.19042.1889 AS runner

COPY --from=builder C:/app/target/release/wm-server.exe C:/wm-server.exe
ENTRYPOINT [ "C:/wm-server.exe" ]
