FROM --platform=linux/amd64 node:18-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    python3 \
    cmake \
    pkg-config \
    alsa-utils \
    pocketsphinx \
    libpocketsphinx-dev \
    sphinxbase-utils \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy package files
COPY package.json pnpm-lock.yaml ./
COPY server/package.json ./server/
COPY pi-client/package.json ./pi-client/

# Install pnpm
RUN npm install -g pnpm

# Install all dependencies first
RUN pnpm install --frozen-lockfile

# Copy source code
COPY . .

# Install packages and their types for server
RUN pnpm add socket.io node-fetch@2 --filter server && \
    pnpm add -D @types/node @types/socket.io @types/node-fetch@2 --filter server

# Install packages and their types for pi-client
RUN pnpm add socket.io-client pocketsphinx --filter pi-client && \
    pnpm add -D @types/node @types/socket.io-client --filter pi-client

# Build TypeScript
RUN pnpm build

# Remove dev dependencies after build
RUN pnpm install --prod

# The CMD will be overridden by docker-compose
CMD ["pnpm", "start:server"] 