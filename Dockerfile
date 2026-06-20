FROM ubuntu:24.04

# Set non-interactive mode to prevent apt from waiting for input
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get install --no-install-recommends -y \
    # Core build tools
    build-essential \
    git \
    ninja-build \
    meson \
    pkgconf \
    # QEMU dependencies
    flex \
    bison \
    libfdt-dev \
    libffi-dev \
    libglib2.0-dev \
    libpixman-1-dev \
    # Python
    python3 \
    python3-venv \
    python3-pip \
    # Additional tools recommended by the official script
    bc \
    ca-certificates \
    ccache \
    locales \
    # Development tools
    vim \
    && rm -rf /var/lib/apt/lists/*

# Set locale (fix potential encoding errors during compilation)
RUN sed -Ei 's,^# (en_US\.UTF-8 .*)$,\1,' /etc/locale.gen && \
    dpkg-reconfigure locales

# Set environment variables
ENV LANG="en_US.UTF-8" \
    LC_ALL="en_US.UTF-8"

# [Optional] Remove PEP 668 restriction to allow direct pip install (as per the official script)
RUN rm -f /usr/lib/python3.*/EXTERNALLY-MANAGED

# Configure ccache (optional, speeds up repeated builds)
# Create symlinks so gcc/cc automatically use ccache
RUN mkdir -p /usr/libexec/ccache-wrappers && \
    ln -s /usr/bin/ccache /usr/libexec/ccache-wrappers/cc && \
    ln -s /usr/bin/ccache /usr/libexec/ccache-wrappers/gcc
ENV PATH="/usr/libexec/ccache-wrappers:$PATH"

WORKDIR /workspace

CMD ["/bin/bash"]
