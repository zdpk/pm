# Dockerfile for PM development environment  
FROM rust:1.82-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    pkg-config \
    libssl-dev \
    libgit2-dev \
    zlib1g-dev \
    cmake \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && apt-get update \
    && apt-get install gh -y

# Set working directory
WORKDIR /workspace

# Copy dependency files
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs and lib.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "pub fn dummy() {}" > src/lib.rs
RUN cargo build --release && rm -rf src

# Copy source code
COPY . .

# Build the project
RUN cargo build --release

# Create development user (non-root)
RUN useradd -m -s /bin/bash developer
RUN chown -R developer:developer /workspace

# Switch to development user
USER developer

# Set up shell environment
RUN echo 'export PATH="/workspace/target/release:$PATH"' >> ~/.bashrc
RUN echo 'alias pm="/workspace/target/release/pm"' >> ~/.bashrc

# Default command
CMD ["/bin/bash"]