# HS-NAS-R1 Panel

海康威视 NAS R1 的 `376×960 @ 56 Hz` 竖向触摸屏状态面板。**完全从零开发的 Rust 原生实现**：`master` 使用纯 Rust 软件渲染，直接驱动 DRM/KMS 显示和 evdev 触摸设备，无任何 GUI 框架依赖。

## 功能

- CPU、内存环形仪表及温度/频率
- 网络上下行速率（`B/s`、`KB/s`、`MB/s`、`GB/s`）及 IPv4
- eMMC、NVMe、HDD 健康、温度、损耗和容量进度条
- 核心服务、Docker 容器和 libvirt 虚拟机状态
- 重启/关机二次确认弹窗
- 四页布局（概况 / 服务 / 虚拟机 / 电源操作），纵向滚动、惯性滚动、横向循环翻页和点击
- DRM 双 dumb buffer + page flip
- 快速指标每 `500 ms` 更新，慢速清单每 `5 s` 更新；动态数据局部重绘，不清空整屏

## 一键安装

发布包是静态链接的 Linux x86_64 二进制。安装脚本会下载并校验最新版本，然后创建 `r1-panel.service`：

```bash
curl -fsSL https://raw.githubusercontent.com/fayfoxcat/HS-NAS-R1-Panel/master/install.sh | sudo bash
```

运行时可选使用系统已有的 `smartctl`、`docker`、`virsh` 和 `systemctl` 获取附加状态；缺少某个命令不会影响面板启动。

## 磁盘健康指标说明

每块磁盘显示一行：型号、类型徽章、健康徽章、元信息和容量进度条。

- **eMMC（系统盘）**：eMMC 没有通电时长计数器（EXT_CSD 不提供、smartctl 也不支持 MMC），因此**不显示使用时长和出厂日期**，显示：
  - 已耗寿命：来自 EXT_CSD `life_time`（0x0=0~10%、0x1=10~20%、… 0x9=90~100%、0xA/0xB=超过额定寿命），显示两个估算值中较大的区间。
  - 健康徽章：来自 `pre_eol_info`（0x01 正常 / 0x02 注意 / 0x03 告警），以它为准。
  - 判断参考：已耗 <50% 属正常使用，50~80% 建议关注，>80% 建议备份数据并留意更换。
- **NVMe/SSD**：smartctl 的 `Percentage Used`（损耗%）、`Power On Hours`（使用时长 h）、`Temperature`。
- **HDD**：SMART 属性 `Power_On_Hours`（使用时长）、`Temperature_Celsius`（温度）、`Reallocated_Sector_Ct`（坏道数，0 为正常）；健康取 `SMART overall-health` 自检结果（正常/注意/告警）。
- 容量进度条表示该盘已挂载分区的文件系统使用率，与健康无关。

## 代码结构

```text
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

`assets/emoji/` 中的 RGBA 图标通过 `include_bytes!` 编译进程序。`assets/emoji.ttf` 只用于 `tools/extract_emoji_bitmaps.py` 重新生成这些图标，不是运行时依赖。

## 构建与验证

Linux/WSL 环境需要 Rust、`musl-tools` 以及 `x86_64-unknown-linux-musl` target：

```bash
cargo fmt --all --check
cargo check --locked --target x86_64-unknown-linux-gnu
cargo test --locked --target x86_64-unknown-linux-musl
cargo build --locked --release --target x86_64-unknown-linux-musl
```

产物：

```text
target/x86_64-unknown-linux-musl/release/r1-panel
```

程序依次尝试系统中的 Droid、DejaVu 和 Noto 字体。也可以通过 `R1_PANEL_FONT=/path/to/font.ttf` 指定字体。

## 截图模式

截图模式不打开 DRM 和 evdev，无 DRM 设备时也可以直接生成 `376×960` BMP 截图：

```bash
R1_PANEL_PAGE=overview ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot overview.bmp
R1_PANEL_PAGE=services R1_PANEL_SCROLL=900 ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot services-bottom.bmp
R1_PANEL_PAGE=vms ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot vms.bmp
R1_PANEL_PAGE=power ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot power.bmp
R1_PANEL_MODAL=reboot ./target/x86_64-unknown-linux-musl/release/r1-panel --screenshot reboot.bmp
```

推送 `v*` tag 会由 GitHub Actions 运行格式检查、测试和 musl release 构建，并发布 `r1-panel` 与 SHA256 校验文件。

## 运行与稳定性

- 快速和慢速更新通道均为容量 1 的有界 channel，旧样本会被覆盖，不会无限积压。
- `smartctl`、`docker`、`virsh`、`systemctl` 等慢命令只在后台采集线程中执行。
- DRM 两个 dumb buffer 在退出时解除映射并销毁；链路维护线程共享停止标志。
- 动态更新不重建页面，只有慢清单或布局变化才触发整页重绘。
- 无触摸、动画和待更新数据时，主循环只进行轻量轮询。

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

当前开发约束见 [会话交接文档.md](会话交接文档.md)。

## 分支说明

- `master`：唯一主线，完全从零开发的 Rust 原生实现，后续开发均在此分支进行。

MIT License
