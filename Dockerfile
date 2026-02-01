FROM ubuntu:24.04

# 設定非互動模式，避免 apt 等待輸入
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get install --no-install-recommends -y \
    # 核心編譯工具
    build-essential \
    git \
    ninja-build \
    meson \
    pkgconf \
    # QEMU 相依套件
    flex \
    bison \
    libfdt-dev \
    libffi-dev \
    libglib2.0-dev \
    libpixman-1-dev \
    # Python 相關
    python3 \
    python3-venv \
    python3-pip \
    # 官方腳本額外建議的工具
    bc \
    ca-certificates \
    ccache \
    locales \
    # 開發工具
    vim \
    && rm -rf /var/lib/apt/lists/*

# 設定語系 (解決編譯時可能的編碼錯誤)
RUN sed -Ei 's,^# (en_US\.UTF-8 .*)$,\1,' /etc/locale.gen && \
    dpkg-reconfigure locales

# 設定環境變數
ENV LANG="en_US.UTF-8" \
    LC_ALL="en_US.UTF-8"

# [選用] 移除 PEP 668 限制，允許直接 pip install (如同官方腳本)
RUN rm -f /usr/lib/python3.*/EXTERNALLY-MANAGED

# 設定 ccache (選用，加速重複編譯)
# 建立符號連結讓 gcc/cc 自動使用 ccache
RUN mkdir -p /usr/libexec/ccache-wrappers && \
    ln -s /usr/bin/ccache /usr/libexec/ccache-wrappers/cc && \
    ln -s /usr/bin/ccache /usr/libexec/ccache-wrappers/gcc
ENV PATH="/usr/libexec/ccache-wrappers:$PATH"

WORKDIR /workspace

CMD ["/bin/bash"]
