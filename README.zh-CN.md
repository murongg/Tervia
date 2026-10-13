# Tervia

[English](README.md) | 简体中文

原生桌面开发工作区，使用 Rust、GPUI 与 GPUI Kit；没有 WebView。终端、Git 管理和轻量文件编辑均接入真实项目目录。优先打磨 macOS，同时提供 Windows / Linux 的平台适配与构建入口。

## 运行

项目通过 `rust-toolchain.toml` 固定 Rust 1.99.0；首次运行会下载对应工具链与依赖。macOS 需要 macOS 15+ 和 Xcode：

```sh
./scripts/run.sh
```

编译后也可以直接打开 `build/Tervia.app`。

Linux 需要 Vulkan 驱动以及 Fontconfig、ALSA、XKB、XCB、Wayland、PipeWire 等开发库；Ubuntu / Debian 安装命令见 `.github/workflows/native.yml`。运行 `cargo run --features native`，或用 `./scripts/linux.sh` 生成压缩包。

Windows 使用 MSVC Rust 工具链和 Visual Studio C++ Build Tools，运行 `cargo run --features native`；PowerShell 中执行 `./scripts/windows.ps1` 生成 ZIP。终端使用系统 ConPTY，默认打开 PowerShell 并保留 Profile。Linux 默认 Bash，macOS 默认 Zsh；设置了 `SHELL` 时优先使用该值。

Git 接入在 macOS 上通过合成仓库测试、原生交互测试与深浅主题界面复查，实际验证了多仓库发现、折叠和滚动、区块暂存、提交、冲突编辑与继续合并。Git 核心通过 Windows GNU / Linux GNU 交叉编译检查；两个系统的图形界面仍需实机验证。CI 配置会在三个系统执行测试、构建及打包，尚未运行远程 CI。

## 可体验的交互

- 窄项目图标栏与展开项目列表；切换项目时保留各自的终端、Tab 和编辑草稿。
- 固定的终端 / Git / 文件 Tab，深色与浅色主题。
- 界面支持简体中文、繁体中文、英语、日语、韩语、法语、德语、西班牙语、葡萄牙语和俄语。默认使用系统首选语言，也可在设置中即时切换；偏好保存到可手动编辑的 TOML 配置文件。新增 Git 文案当前主要覆盖简体中文与英语，其他语言部分使用英语。
- 终端识别 Claude / Codex / Gemini / Grok / Hermes / OpenCode / OpenClaw，显示对应品牌图标与标签；退出 Agent 后恢复 Shell 标签。macOS / Linux 使用前台进程组，Windows 使用当前 Shell 的子进程树；Windows 中的后台 Agent 也可能影响标签。不会扫描终端对话内容来识别程序。
- 真实 PTY 终端：加载本机登录 / 交互 Shell 配置，新建、切换、关闭、分屏、滚动历史、键盘输入、UTF-8/输入法、ANSI/真彩色、全屏备用缓冲区。
- 鼠标拖选后 ⌘C 复制，⌘V 粘贴；支持括号粘贴、控制键与应用光标键。全屏程序启用鼠标报告时，按住 Shift 拖选文本。
- 终端提示符与输入由 Shell 自己绘制，兼容已有提示符主题的基础协议；没有独立输入框。
- 真实 Git 管理：变更 / 历史 / 分支 / 远程 / Stash，原生行内 / 并排 Diff 和区块暂存。
- 文件页显示所选项目的目录树：按需读取、虚拟化长列表、展开 / 折叠，状态按项目保留。原生文件监控在后台暂停，回到前台立即刷新并继续自动同步目录和未修改的编辑内容；磁盘变化时保留未保存草稿。可从文件树、Git 或系统文件选择器打开文件；支持多个标签、语法高亮、查找、保存和右上角刷新。
- 文件标签支持关闭按钮、中键关闭、右键关闭其他 / 右侧 / 全部、拖动排序、Ctrl+Tab 切换和重新打开关闭的标签；关闭未保存文件前确认。
- 欢迎页打开 / 克隆仓库，项目栏添加真实目录，快捷操作。
- 带功能分类侧栏的独立原生设置窗口：选择已安装的常用等宽字体、调整终端 / 代码字号与界面字号、即时预览、恢复默认。

启动时显示打开 / 克隆仓库的欢迎页。终端在项目目录中启动，Git 操作使用本机 Git CLI，文件保存会写入磁盘。切换项目保留各自的终端、提交说明和编辑草稿；偏好已持久化，项目列表暂未持久化。

## 配置文件

首次启动自动生成带注释的 `config.toml`，可通过**设置侧栏左下角的 config.toml** 定位：

| 平台 | 默认位置 |
| --- | --- |
| macOS | `~/Library/Application Support/tervia/config.toml` |
| Linux | `$XDG_CONFIG_HOME/tervia/config.toml`，未设置时为 `~/.config/tervia/config.toml` |
| Windows | `%APPDATA%\tervia\config.toml` |

可通过环境变量 `TERVIA_CONFIG_FILE` 指定绝对文件路径。配置属于整个应用，不随项目目录变化。

顶层按功能域划分大类：当前设置归入 `ui`，字体使用其下的 `fonts` 子类。今后终端、编辑器、Git、工作区等设置可以分别扩展为独立的顶层大类，避免所有配置平铺在一起。

```toml
[ui]
language = "system" # system、zh-CN、zh-TW、en、ja、ko、fr、de、es、pt、ru
theme = "dark"      # dark 或 light

[ui.fonts]
code_family = "Menlo" # 终端与编辑器共用的已安装字体
code_size = 14        # 11–24
ui_size = 13          # 11–16
```

代码字体默认值：macOS 为 Menlo、Windows 为 Consolas、Linux 为 DejaVu Sans Mono。未填写的配置项使用默认值；完整说明见[带注释的模板](assets/config.toml)。

设置窗口、主题切换和恢复默认都会即时生效并保存到同一文件。保存保留注释与未知配置项，使用原子替换，并保留 dotfiles 符号链接。手动编辑后重启 Tervia 生效；如果运行期间文件被修改，应用会停止保存并提示重启，避免覆盖外部编辑。

TOML 语法、配置值或读取出错时，使用默认设置启动并显示配置错误，不覆盖原文件。可在设置中查看错误详情并定位文件。保存失败时修改仍在本次运行中生效，设置窗口会提示错误；修复文件或权限后重启即可。

## Git 管理

- 打开真实工作目录、初始化仓库、克隆到新目录；可直接打开已有 linked worktree。普通父目录自动发现多个子仓库，并在侧栏按工作区分组切换；支持 `.git` 目录和 `.git` 文件。
- 历史、变更文件、分支、侧栏、Stash 与原始文本详情按可见行虚拟渲染。文件夹与变更 / 仓库 / 分支 / 标签分组可以折叠，状态按项目保留。
- 保留整个窗口的连续灰蓝渐变；折叠栏整行 hover，计数右侧有内间距。
子仓库扫描在后台进行，最多遍历 8 层、8,192 个目录和 128 个仓库，按时间预算中断并显示范围上限提示。跳过 `.git` 元数据、node_modules、target、build、.venv 与 .cache，不跟随子目录符号链接；遗漏的仓库可以直接打开或重新扫描。

- 分别显示 index / worktree 状态，同一文件可以同时出现在已暂存和未暂存组；支持新增、修改、删除、重命名与冲突。
- 原生虚拟化 Diff、语法高亮、行内 / 并排、按文件或区块暂存和取消暂存。区块应用前检查快照，保留 CRLF 与行尾空白。
- 多行提交说明、普通提交与 amend；失败保留说明，提交只消费 index。保留本机 hooks 与签名行为。
- 提交列表、作者 / 说明 / ref / hash 筛选、分页加载和提交详情；当前最多加载 2000 条，不含提交图。
- 创建 / 切换 / 重命名 / 删除本地分支、跟踪远程分支、创建 / 删除标签。
- Worktree 归属同一项目，在 Git 侧栏按折叠列表切换，各目录分别保留终端、标签和草稿。支持从新分支、已有分支或游离 HEAD 创建，打开、锁定 / 解锁、安全移除及清理失效登记。移除保留分支，检查 Git 改动和未保存编辑内容，不提供强制删除。
- 添加 / 删除 remote、更新 URL、fetch、快进 pull、merge / rebase pull、push 和首次设置 upstream。
- Stash 保存（包含未跟踪文件）、预览、apply、pop、drop；操作绑定对象 ID，删除在 Git 引用锁内校验目标。当前支持 files 引用存储；其他存储格式会保留条目并提示终端处理。
- merge / rebase 状态、冲突三阶段内容查看、结果编辑、标记解决、继续和中止；删除文件也可作为解决结果。
- Git 页每 3 秒在后台刷新，切入页面或操作完成时刷新；后台结果保留项目归属并拒绝过期 Diff / 文件读取。

使用现有 Git 配置、SSH、credential helper 和签名程序；需要交互认证时可在终端完成。编辑器限 UTF-8 文本与 4 MiB 文件；非 UTF-8 Diff 给出提示，整文件暂存仍保留原始字节。Git 输出上限 8 MiB。交互式 rebase、PR / Issue / CI 集成与项目 / 会话持久化属于后续工作。

创建终端时会移除启动器继承的 `NO_COLOR`，并声明 `xterm-256color` / `truecolor` 能力，避免从无色命令环境启动应用时让 TUI 自动关闭颜色。用户的 Shell 配置或单条命令仍可显式设置 `NO_COLOR`。

当前终端主要实现标准 xterm/VT 交互；图片协议、完整 Kitty 键盘协议、终端搜索和持久会话尚未实现。兼容性需要按具体插件和程序验证。

## 快捷键

| macOS | Windows / Linux | 操作 |
| --- | --- | --- |
| ⌘B | Ctrl+Shift+B | 展开 / 收起项目栏 |
| ⌘1 / ⌘2 / ⌘3 | Ctrl+1 / Ctrl+2 / Ctrl+3 | 终端 / Git / 文件 |
| ⌘T | Ctrl+Shift+T | 新建终端 |
| ⌘S | Ctrl+S | 保存当前文件到磁盘 |
| ⌘W | Ctrl+W | 关闭当前文件标签 |
| Ctrl+Tab / Ctrl+Shift+Tab | Ctrl+Tab / Ctrl+Shift+Tab | 下一个 / 上一个文件标签 |
| ⌘Shift+T | Ctrl+Shift+T | 重新打开关闭的文件标签 |
| ⌘F | Ctrl+F | 编辑器内查找 |
| ⌘⇧P | Ctrl+Shift+P | 快捷操作 |
| ⌘, | Ctrl+, | 打开独立设置窗口 |
| ⌘C / ⌘V | Ctrl+Shift+C / Ctrl+Shift+V | 复制终端选区 / 粘贴到终端 |
| ⌘K | Ctrl+Shift+K | 清空获得焦点的终端屏幕与滚动历史，并请求程序重绘 |
| Ctrl+K | Ctrl+K | 发送给 Shell / TUI，保留程序原有快捷键 |
| ⇧PageUp / ⇧PageDown | Shift+PageUp / Shift+PageDown | 终端历史翻页 |
| Esc | Esc | 终端焦点下发送给程序；其他区域优先关闭弹层，再关闭窗口 |

## 验证

```sh
cargo test --no-default-features
cargo test --features native
cargo test --features ui-tests
cargo fmt --all -- --check
cargo clippy --features native --all-targets -- -D warnings
cargo build --features native
```

## 结构

- `src/config.rs`、`assets/config.toml`：用户配置、校验和保留注释的持久化。
- `src/state.rs`：独立于 GPUI 的真实项目、终端与编辑草稿状态。
- `src/terminal.rs`、`src/pty.rs`：终端解析、按键编码、选区和真实 PTY 生命周期。
- `src/locale.rs`、`assets/locales.json`：系统语言匹配和界面文案。
- `src/agent.rs`：前台 Agent 的纯路径识别规则。
- `src/platform.rs`、`src/process.rs`：平台字体、Shell、快捷键和后台进程查询。
- `src/ui/console.rs`：GPUI 原生终端渲染、输入法与交互。
- `src/git/`：Git 命令与输出、状态解析、Diff、仓库快照和写操作。
- `src/files/`：磁盘读写、范围检查和异步读取版本保护。
- `src/ui/`：应用外壳、终端、Git、文件视图和视觉样式。
- `tests/git.rs`：只使用合成临时仓库与本地 bare remote 的 Git 集成测试。
- `tests/reads.rs`：异步重新加载与草稿保护测试。
- `tests/workspace.rs`：项目隔离与终端 / 工作区生命周期测试。
- `PRODUCT.md`、`DESIGN.md`：已确认的产品边界与界面设计。
