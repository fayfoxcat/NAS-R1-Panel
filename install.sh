#!/bin/bash
# HS-NAS-R1 Panel — install the latest native Rust release.
# Usage: curl -fsSL https://raw.githubusercontent.com/fayfoxcat/HS-NAS-R1-Panel/master/install.sh | sudo bash

set -euo pipefail

REPO="https://github.com/fayfoxcat/HS-NAS-R1-Panel"
INSTALL_DIR="/opt/r1-panel"
BIN="${INSTALL_DIR}/r1-panel"
SERVICE="/etc/systemd/system/r1-panel.service"

if [ "${EUID}" -ne 0 ]; then
    echo "错误：请使用 root 或 sudo 运行。" >&2
    exit 1
fi

if [ "$(uname -m)" != "x86_64" ]; then
    echo "错误：当前发布包仅支持 Linux x86_64。" >&2
    exit 1
fi

for command in curl sha256sum systemctl; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        echo "错误：缺少命令 ${command}。" >&2
        exit 1
    fi
done

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

echo "[1/3] 下载并校验最新版本..."
curl -fL "${REPO}/releases/latest/download/r1-panel" -o "${tmp_dir}/r1-panel"
curl -fL "${REPO}/releases/latest/download/r1-panel.sha256" -o "${tmp_dir}/r1-panel.sha256"
(cd "${tmp_dir}" && sha256sum -c r1-panel.sha256)

echo "[2/3] 安装到 ${BIN}..."
install -d -m 755 "${INSTALL_DIR}"
install -m 755 "${tmp_dir}/r1-panel" "${BIN}.new"
mv -f "${BIN}.new" "${BIN}"

echo "[3/3] 配置 systemd 服务..."
cat >"${SERVICE}" <<EOF
[Unit]
Description=HS-NAS-R1 native display panel
After=local-fs.target systemd-udev-settle.service
Wants=systemd-udev-settle.service

[Service]
Type=simple
WorkingDirectory=${INSTALL_DIR}
ExecStart=${BIN}
Environment=RUST_LOG=info
Restart=on-failure
RestartSec=2
KillSignal=SIGTERM
TimeoutStopSec=10

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable r1-panel.service >/dev/null
systemctl restart r1-panel.service

echo "安装完成：${BIN}"
echo "查看状态：systemctl status r1-panel.service"
echo "查看日志：journalctl -u r1-panel.service -f"
echo "smartctl、docker 和 virsh 为可选指标来源，本脚本不会安装或升级系统软件包。"
