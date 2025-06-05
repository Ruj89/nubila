FROM ruj89/debian

USER ruj89
RUN sudo apt-get update
RUN sudo apt-get -y upgrade
RUN sudo apt-get install -y curl
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
RUN bash -c ". /home/ruj89/.nvm/nvm.sh && \
             nvm install --lts && \
             corepack enable pnpm"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN bash -lc "rustup target add wasm32-unknown-unknown"

RUN sudo apt-get install -y make clang jq git

RUN bash -lc "cargo install wasm-bindgen-cli"
RUN curl -Lo- https://github.com/WebAssembly/binaryen/releases/download/version_123/binaryen-version_123-x86_64-linux.tar.gz | sudo tar -xz -C /usr --strip-components=1
RUN bash -lc "cargo install --git https://github.com/r58Playz/wasm-snip"

CMD ["bash", "-c", "trap 'exit 0' SIGTERM; while true; do sleep 1; done"]