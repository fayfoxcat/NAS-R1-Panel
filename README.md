# HS-NAS-R1 Panel

海康威视 NAS R1 的 `376×960` 竖向触摸屏状态面板。`master` 是 Rust 原生 DRM/KMS 主线，直接驱动显示和 evdev 触摸设备，不依赖 Cog、WPE WebKit 或 LVGL。旧 Go + Cog 实现完整保存在 `cog` 分支。

## 功能

- CPU、内存、温度、频率和使用率环形图
- 网络上下行速率（`B/s`、`KB/s`、`MB/s`、`GB/s`）及 IPv4
- eMMC、NVMe、HDD 健康、温度、损耗和容量进度条
- 核心服务、Docker 容器和 libvirt 虚拟机状态
- 重启/关机二次确认
- 纵向滚动、惯性滚动、横向循环翻页和点击
- DRM 双缓冲 + page flip
- 快速指标每 `500 ms` 更新，慢速清单每 `5 s` 更新；动态数据局部重绘

## 一键安装

发布包是静态链接的 Linux x86_64 二进制。安装脚本会下载并校验最新版本，然后创建 `r1-panel.service`：

```bash
curl -fsSL https://raw.githubusercontent.com/fayfoxcat/HS-NAS-R1-Panel/master/install.sh | sudo bash
```

运行时可选使用系统已有的 `smartctl`、`docker`、`virsh` 和 `systemctl` 获取附加状态；缺少某个命令不会影响面板启动。

## 代码结构

```text
rust-lvgl-panel/
├── assets/emoji/          # 编译进二进制的界面图标
├── src/main.rs            # 入口、事件循环和电源操作
├── src/display.rs         # DRM/KMS、双缓冲和链路维护
├── src/input.rs           # evdev 触摸与手势
├── src/interaction.rs     # 滚动、惯性和切页动画
├── src/metrics.rs         # 指标模型与采集器
├── src/metrics_worker.rs  # 500 ms 快采样、5 s 慢采样
├── src/render.rs          # Rust 软件渲染器
└── src/view.rs            # 页面布局和增量绘制
```

旧 Go/Cog、Web 前端和 LVGL FFI 文件不再位于 `master`。需要查看或维护旧实现时切换到 `cog` 分支。

## 构建与验证

Linux/WSL 环境需要 Rust、`musl-tools` 以及 `x86_64-unknown-linux-musl` target：

```bash
cd rust-lvgl-panel
cargo fmt --all --check
cargo check --locked --target x86_64-unknown-linux-gnu
cargo test --locked --target x86_64-unknown-linux-musl
cargo build --locked --release --target x86_64-unknown-linux-musl
```

产物：

```text
rust-lvgl-panel/target/x86_64-unknown-linux-musl/release/r1-panel
```

无 DRM 设备时可以直接生成 `376×960` BMP 截图：

```bash
R1_PANEL_PAGE=overview ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot overview.bmp
R1_PANEL_PAGE=services R1_PANEL_SCROLL=900 ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot services-bottom.bmp
R1_PANEL_MODAL=reboot ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot reboot.bmp
```

推送 `v*` tag 会由 GitHub Actions 运行格式检查、测试和 musl release 构建，并发布 `r1-panel` 与 SHA256 校验文件。

## 开发机到 NAS 的安全测试流程

测试部署应使用带 SHA256 前 8 位的新文件名，不覆盖或删除旧版本。停止旧进程前必须重新核对进程命令行，再优先发送 `SIGTERM`：

```text
scp r1-panel-rust-<hash> <nas>:/opt/r1-panel/
ssh <nas>
cd /opt/r1-panel
chmod 755 r1-panel-rust-<hash>
ps -ef | grep '[r]1-panel'
kill -TERM <已核实的旧 PID>
nohup env RUST_LOG=info ./r1-panel-rust-<hash> >rust-<hash>.log 2>&1 &
```

详细实现说明见 [rust-lvgl-panel/README.md](rust-lvgl-panel/README.md)，当前开发约束见 [会话交接文档.md](会话交接文档.md)。

MIT License
