FROM ubuntu:latest

# Install sudo, curl, and other dependencies
RUN apt-get update && \
    apt-get install -y \
    sudo \
    build-essential nano git curl

RUN useradd -m -s /bin/bash ubuntu && \
    echo "ubuntu ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/ubuntu && \
    chmod 0440 /etc/sudoers.d/ubuntu

USER ubuntu
WORKDIR /home/ubuntu

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup && \
    sh /tmp/rustup -y && \
    rm /tmp/rustup

ENV PATH="/home/ubuntu/.cargo/bin:${PATH}"
RUN rustup default stable

RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
RUN bash -ic "nvm install v22.18.0"
RUN bash -ic "npm install -g @anthropic-ai/claude-code"
