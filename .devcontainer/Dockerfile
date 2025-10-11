FROM ubuntu:latest

# Install sudo, curl, and other dependencies
RUN apt-get update && \
    apt-get install -y \
    sudo \
    rustup build-essential nano git curl

RUN echo "ubuntu ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/ubuntu && \
    chmod 0440 /etc/sudoers.d/ubuntu

USER ubuntu
WORKDIR /home/ubuntu


RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
RUN bash -ic "nvm install v22.18.0"
RUN bash -ic "npm install -g @anthropic-ai/claude-code"
RUN rustup default stable
