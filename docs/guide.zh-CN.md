# Tervia 使用与开发指南

[English](guide.md) | 简体中文 · [返回产品介绍](../README.zh-CN.md)

这里保留完整的使用、配置、构建与发布说明。文中的命令均在仓库根目录执行。

## 运行

项目通过 `rust-toolchain.toml` 固定 Rust 1.99.0；首次运行会下载对应工具链与依赖。macOS 需要 macOS 15+ 和 Xcode：

```sh
./scripts/run.sh
```

脚本默认编译 release 构建，编译后也可以直接打开 `build/Tervia.app`；该构建支持在线更新。需要开发构建时运行 `cargo run --features native`。

Linux 需要 Vulkan 驱动以及 Fontconfig、ALSA、XKB、XCB、Wayland、PipeWire 等开发库；Ubuntu / Debian 安装命令见 `.github/workflows/native.yml`。运行 `cargo run --features native`，或用 `./scripts/linux.sh` 生成压缩包。

Windows 使用 MSVC Rust 工具链和 Visual Studio C++ Build Tools，运行 `cargo run --features native`；PowerShell 中执行 `./scripts/windows.ps1` 生成 ZIP。终端使用系统 ConPTY，默认打开 PowerShell 并保留 Profile。Linux 默认 Bash，macOS 默认 Zsh；设置了 `SHELL` 时优先使用该值。

Windows 应用 manifest 由 Tervia 统一提供，通过[固定版本 GPUI 的局部补丁](../vendor/gpui-pre/PATCH.md) 避免重复资源，同时保留 DPI、Common Controls v6、长路径和 UTF-8 设置。Windows CI 会检查链接后的 EXE 是否包含这些设置。

Git 接入在 macOS 上通过合成仓库测试、原生交互测试与深浅主题界面复查，实际验证了多仓库发现、折叠和滚动、区块暂存、提交、冲突编辑与继续合并。Git 核心通过 Windows GNU / Linux GNU 交叉编译检查；两个系统的图形界面仍需实机验证。CI 配置会在三个系统执行测试、构建及打包，尚未运行远程 CI。

## 自动发版

[发版脚本](../scripts/release.sh) 使用 POSIX shell、Git 和 Cargo，Windows 下使用 Git Bash。先修改 `Cargo.toml` 的 `package.version`，运行 `cargo check` 同步 `Cargo.lock` 并缓存依赖，提交所有改动后再发版。脚本读取已提交的版本，不会自动递增版本号。

```sh
git fetch --tags origin
sh scripts/release.sh --dry-run  # 预览 changelog，不修改文件或 Git 引用
sh scripts/release.sh --push     # 生成、提交、打标签并推送到 origin
```

不加参数时只创建本地版本，并打印推送命令。脚本根据最近可达的 `v*` 版本标签以来的非合并提交生成 `CHANGELOG.md`，首次发版使用完整历史。按 Conventional Commits 将提交归为新功能、修复、性能、文档、破坏性变更和其他变更，保留已有 changelog。随后创建 `chore(release): v<版本号>` 提交，再创建指向该提交的附注标签，标签说明包含本次 changelog。

脚本要求工作区干净、位于分支上、历史完整、lockfile 版本一致，且目标标签尚不存在。`--push` 会先同步 origin 标签，再原子推送当前分支和版本标签。Hook 或推送失败时保留生成的文件、提交与标签供检查；推送失败会打印重试命令。保留现有 Git hooks 与签名设置。

推送标签后触发[发布工作流](../.github/workflows/release.yml)，复用 macOS、Linux、Windows 的原生 CI 检查与 release 构建；三平台全部成功后，用附注标签中的 changelog 创建 GitHub Release，并上传带版本号的平台压缩包与 `SHA256SUMS`。手动推送轻量标签时仍使用 GitHub 自动生成的发布说明。`v0.2.0-beta.1` 等标签会标记为预发布，`Cargo.toml` 也需使用相同版本。GitHub 发布使用内置 `GITHUB_TOKEN`，macOS 发版还需下方的 Apple Secrets；已有 Release 不会被覆盖。

### macOS 正式签名与公证

推送发版标签前，在仓库 **Settings → Secrets and variables → Actions** 配置：

| Secret | 内容 |
| --- | --- |
| `MACOS_CERTIFICATE` | 包含一个 **Developer ID Application** 证书及对应私钥的 `.p12`，转成 Base64。 |
| `MACOS_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码。 |
| `APPLE_ID` | Apple 开发者账号邮箱。 |
| `APPLE_TEAM_ID` | 10 位开发者团队 ID。 |
| `APPLE_APP_SPECIFIC_PASSWORD` | 用于公证的 App 专用密码，不是 Apple 账号登录密码。 |

需要 Apple Developer Program 会员。从钥匙串访问中将证书与对应私钥一起导出，再执行 `base64 -i DeveloperID.p12 | pbcopy`，将结果粘贴到 Secret。证书必须属于 `APPLE_TEAM_ID` 指定团队；不接受 Apple Development 或 Mac App Store 分发证书。参考 [Apple Developer ID 说明](https://developer.apple.com/developer-id/)与[公证文档](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)。

[sign.sh](../scripts/sign.sh) 将证书导入临时钥匙串，启用 Hardened Runtime 和安全时间戳签名，提交 ZIP 给 Apple 公证，最多等待 30 分钟，只有状态为 `Accepted` 才附加票据并执行 Gatekeeper 检查，随后工作流生成最终 macOS ZIP。缺少凭据、签名无效、公证拒绝或超时、票据检查失败都会终止发版；公证失败时尽可能输出 Apple 诊断日志。成功或失败后都会清理临时证书、凭据与钥匙串，无需额外配置钥匙串密码 Secret。

目标为 `main` 的 PR 通过 [CI 工作流](../.github/workflows/ci.yml) 在 macOS、Linux、Windows 上运行格式检查、测试和 Clippy。推送到 `main` 后运行[原生构建](../.github/workflows/native.yml)，这些构建和本地开发构建使用 ad-hoc 签名。本地公证时，在装有 Xcode 的 Mac 上导出同名的五个环境变量，打包后执行 `sh scripts/sign.sh build/Tervia.app`。`sh scripts/sign.sh --check` 只检查必需值是否存在、团队 ID 格式是否正确，不连接 Apple 或验证证书有效性。

Linux 仍需安装上文所述运行库。每个压缩包只对应其 CI runner 的架构，macOS 文件名会注明架构。

## 在线更新

默认启动时在后台检查 GitHub Releases 的最新稳定版本，运行期间**每 60 分钟自动检查一次**。在**设置 → 在线更新**中可开启或关闭自动检查、选择检查间隔、手动检查、查看更新说明，并确认**下载并安装**。发现新版本时，主窗口右上角显示带圆点的更新图标，点击即可确认下载并安装。下载时该图标变为圆形进度条；下载大小未知或正在校验、安装时显示旋转状态，安装完成后变为重启图标。失败会在发起更新的窗口弹出原因，并保留更新供重试；安装完成后点击**重启 Tervia**使新版本生效，也可稍后自行重启。

间隔从上一次检查结束时计算，手动检查和失败的检查也会重新计算下一次检查时间。检查失败后按设定间隔重试。设置中的修改立即生效；手动编辑 `config.toml` 的 `[updates]` 后重启生效。`interval_minutes` 支持 **5–10080 分钟（7 天）**，自定义分钟数会保留在设置菜单中。关闭自动检查后仍可手动检查；确认或安装期间暂停检查，安装完成后停止检查直到重启。自动检查仅提示新版本，下载和安装始终需要确认。

更新严格匹配操作系统与 CPU 架构，并使用检查时获取的 SHA-256 摘要校验下载包。macOS 整体替换已签名、公证的 `.app`，安装前检查签名和 Gatekeeper；Windows、Linux 替换发布包中的可执行文件。安装目录需要当前用户的写入权限，不会自动提权。校验或安装失败时保留原应用。

重启前必须保存所有项目和 worktree 中的编辑草稿，并等待保存、Git 等操作完成；确认重启会结束全部终端会话，回到欢迎页。开发构建支持自动和手动检查，但不会安装更新。自动更新功能从包含此功能的发布版本开始可用，之前的版本需先手动升级。

## 可体验的交互

- 窄项目图标栏与展开项目列表；切换项目时保留各自的终端、Tab 和编辑草稿。
- 固定的终端 / Git / 文件 Tab，深色与浅色主题。在 Git 或文件页，可通过分支菜单旁的终端图标、视图菜单或 ⌘J / Ctrl+Shift+J 展开可调整高度的底部终端，共用正在运行的会话；新建或分屏时保留当前文档模块。
- 界面支持简体中文、繁体中文、英语、日语、韩语、法语、德语、西班牙语、葡萄牙语和俄语。默认使用系统首选语言，也可在设置中即时切换；偏好保存到可手动编辑的 TOML 配置文件。新增 Git 文案当前主要覆盖简体中文与英语，其他语言部分使用英语。
- 终端识别 Claude / Codex / Gemini / Grok / Hermes / OpenCode / OpenClaw，显示对应品牌图标与标签；退出 Agent 后恢复 Shell 标签。macOS / Linux 使用前台进程组，Windows 使用当前 Shell 的子进程树；Windows 中的后台 Agent 也可能影响标签。不会扫描终端对话内容来识别程序。
- 真实 PTY 终端：加载本机登录 / 交互 Shell 配置，新建、切换、关闭、分屏、滚动历史、键盘输入、UTF-8/输入法、ANSI/真彩色、全屏备用缓冲区。
- 鼠标拖选后 ⌘C 复制，⌘V 粘贴；支持括号粘贴、控制键与应用光标键。全屏程序启用鼠标报告时，按住 Shift 拖选文本。
- ⌘点击终端中的文件路径（Windows / Linux 为 Ctrl 点击），可打开文件并跳转到对应行列，支持 `src/main.rs:42:7`、引号内含空格的路径和当前项目内的绝对路径。相对路径以所选项目目录为基准；普通拖选和 TUI 鼠标操作仍可使用。
- 终端搜索支持 ⌘F（Windows / Linux 为 Ctrl+Shift+F）或工具栏搜索图标，搜索输出与滚动历史中的文字。小写查询忽略大小写，含大写的查询区分大小写；Enter / Shift+Enter 前后定位匹配，Esc 返回 Shell，最多显示 5,000 个匹配。后台终端发出铃声时，会话和所属项目显示请求关注标记；聚焦窗格后清除。
- 终端提示符与输入由 Shell 自己绘制，兼容已有提示符主题的基础协议；没有独立输入框。
- 真实 Git 管理：变更 / 历史 / 分支 / 远程 / Stash，原生行内 / 并排 Diff 和区块暂存。
- 文件页显示所选项目的目录树：按需读取、虚拟化长列表、展开 / 折叠，状态按项目保留。原生文件监控在后台暂停，回到前台立即刷新并继续自动同步目录和未修改的编辑内容；磁盘变化时保留未保存草稿。可从文件树、Git 或系统文件选择器打开文件；支持多个标签、语法高亮、查找、保存和右上角刷新。
- 文件编辑器与 Git Diff 共用语言识别，支持 Rust、JavaScript / JSX、TypeScript / TSX、Python、Go、C / C++、C#、Java、Kotlin、Swift、HTML / CSS、JSON / YAML / TOML、Shell、SQL、Markdown 等内置语法。可识别 Makefile、CMakeLists.txt、Cargo.lock 等常见文件名；Markdown 代码块及网页、模板中的内嵌代码使用各自的语法高亮。
- 文件树单击使用临时预览标签，双击或编辑后固定；⌘P（Windows / Linux 文件页 Ctrl+P）快速查找文件，⌘⇧F / Ctrl+Shift+F 搜索项目文本。方向键选择、Enter 打开、Esc 返回目录树；搜索结果定位到对应行列。Markdown 支持阅读 / 源码切换，常见图片格式使用原生只读预览（沿用 4 MiB 读取上限）。搜索遵循忽略规则，跳过依赖目录、图片和二进制文件，并提示未完整或超限的结果。
- 文件标签支持关闭按钮、中键关闭、右键关闭其他 / 右侧 / 全部、拖动排序、Ctrl+Tab 切换和重新打开关闭的标签；关闭未保存文件前确认。
- 在文件源码或 Git Diff 中选中代码，点击**复制上下文**或按 ⌘⇧C / Ctrl+Shift+C，可复制项目相对路径、起止行号和代码。文件页使用当前草稿；Diff 使用画面中的源码，标明 `original` / `modified`，并分别输出不同版本和不连续的片段，支持拖选文字和通过行号选择整行。Markdown 需切换到源码视图；图片预览不提供代码上下文。
- 欢迎页打开 / 克隆仓库，项目栏添加真实目录，快捷操作。
- 带功能分类侧栏的独立原生设置窗口：选择已安装的常用等宽字体、调整终端 / 代码字号与界面字号、即时预览、恢复默认。

启动时恢复项目列表、顺序、当前工作目录和项目栏状态；没有可用项目时显示打开 / 克隆仓库的欢迎页。模块、底部终端布局、终端标签数量与选中项、打开文件顺序和源码编辑位置也会恢复。未保存草稿连同原来的磁盘版本一并保留；干净文件在后台读取最新内容。终端标签在所属项目目录重建为 Shell，退出前运行的进程和 AI 对话不会自动续接。Git 使用本机 Git CLI，保存文件会写入磁盘。

## 项目管理

侧栏搜索按钮、文件菜单及 Cmd+Option+P（Windows/Linux 为 Ctrl+Alt+P）打开全局项目切换器。支持搜索名称、路径和模糊缩写，直接进入项目或已知工作目录/worktree。空查询优先显示收藏项目，再按最近访问排列；“返回上一个项目”恢复其上次使用的工作目录。

通过星标或右键菜单收藏项目。在收藏组或普通项目组内拖动可调整顺序，右键“上移/下移”提供相同操作。侧栏顺序在项目切换时保持稳定，并在重启后恢复。

“关闭项目”先将草稿、阅读位置和布局保存到恢复文件，再经具名确认结束该项目的终端会话；恢复保存失败时保留项目和终端。关闭前需等待 Git/文件操作完成。“最近关闭的项目”可从侧栏、文件菜单和欢迎页进入；重开后恢复编辑上下文，按需启动新的 Shell。关闭项目不会修改磁盘上的项目文件。

## 配置文件

首次启动自动生成带注释的 `config.toml`，可通过**设置侧栏左下角的 config.toml** 定位：

| 平台 | 默认位置 |
| --- | --- |
| macOS | `~/Library/Application Support/tervia/config.toml` |
| Linux | `$XDG_CONFIG_HOME/tervia/config.toml`，未设置时为 `~/.config/tervia/config.toml` |
| Windows | `%APPDATA%\tervia\config.toml` |

可通过环境变量 `TERVIA_CONFIG_FILE` 指定绝对文件路径。配置属于整个应用，不随项目目录变化。

工作恢复状态保存在同目录的 `workspace.json`，使用 v2 格式并兼容读取旧 v1 项目历史。普通变化每 300 ms 合并保存，切换项目和退出时立即刷新。目录暂时不可用时，未保存草稿仍保留在状态文件中；每个目录最多恢复 64 个终端标签和 512 个打开文件。无效、超限或被其他实例修改的状态会提示错误，保留原文件。

顶层按功能域划分大类：界面设置归入 `ui`，字体使用其下的 `fonts` 子类，文件列表分组使用 `files`，自动更新检查使用独立的 `updates` 类。今后终端、编辑器、Git、工作区等设置可以分别扩展为独立的顶层大类，避免所有配置平铺在一起。

```toml
[ui]
language = "system" # system、zh-CN、zh-TW、en、ja、ko、fr、de、es、pt、ru
theme = "dark"      # dark 或 light

[ui.fonts]
code_family = "Menlo" # 终端与编辑器共用的已安装字体
code_size = 14        # 11–24
ui_size = 13          # 11–16

[files]
nesting = false       # 在文件列表中分组显示关联文件

[updates]
auto_check = true
interval_minutes = 60 # 5–10080，默认 1 小时
```

代码字体默认值：macOS 为 Menlo、Windows 为 Consolas、Linux 为 DejaVu Sans Mono。未填写的配置项使用默认值；完整说明见[带注释的模板](../assets/config.toml)。

在**设置 → 界面 → 文件列表 → 关联文件嵌套**中开启同目录关联文件分组。内置规则参考 [antfu 的 File Nesting Config](https://github.com/antfu/vscode-file-nesting-config)，覆盖 `.env`、Docker、包管理、Rust、Python 和常见源码/测试文件。分组默认收起，点击箭头展开，点击文件名仍然打开文件；开关对所有项目即时生效，默认关闭。

设置窗口、主题切换和恢复默认都会即时生效并保存到同一文件。保存保留注释与未知配置项，使用原子替换，并保留 dotfiles 符号链接。手动编辑后重启 Tervia 生效；如果运行期间文件被修改，应用会停止保存并提示重启，避免覆盖外部编辑。

TOML 语法、配置值或读取出错时，使用默认设置启动并显示配置错误，不覆盖原文件。可在设置中查看错误详情并定位文件。保存失败时修改仍在本次运行中生效，设置窗口会提示错误；修复文件或权限后重启即可。

## Git 管理

- 打开真实工作目录、初始化仓库、克隆到新目录；可直接打开已有 linked worktree。打开父文件夹后保留该目录作为一个工作区根目录。侧栏按真实层级同时显示普通目录和仓库，逐层按需读取，并只渲染可见行。箭头展开目录，名称切换到对应的实际工作目录；Git 图标和分支仅作为仓库目录的附加信息。折叠保留当前目录，并在父目录下显示其相对路径。侧栏收起为图标时，弹出同样的目录树，支持方向键、Enter 和 Esc。各目录独立保留终端、草稿和当前模块，历史记住当前目录及根目录折叠状态；手动展开没有深度限制，只有自动 Git 识别限制为 3 层；支持 `.git` 目录和 `.git` 文件。
- 历史、变更文件、分支、侧栏、Stash 与原始文本详情按可见行虚拟渲染。文件夹与变更 / 仓库 / 分支 / 标签分组可以折叠，状态按项目保留。
- 保留整个窗口的连续灰蓝渐变；折叠栏整行 hover，计数右侧有内间距。
子仓库扫描在后台进行，最多检查所选文件夹下面 3 层子目录、8,192 个目录和 128 个 Git 工作目录，时间预算为 3 秒。未完整扫描时会说明触发的是时间、深度、目录数量、Git 工作目录数量上限，还是目录无法读取；同一仓库的 worktree 仍归为一个仓库。扫描跳过 `.git` 元数据、node_modules、target、build、.venv 与 .cache，不跟随子目录符号链接；遗漏的仓库可以直接打开或重新扫描。

- 分别显示 index / worktree 状态，同一文件可以同时出现在已暂存和未暂存组；支持新增、修改、删除、重命名与冲突。
- 未暂存分组支持撤销单个文件或全部修改，确认后还原到暂存区内容，保留已暂存修改。未跟踪文件支持单个或整组移到系统废纸篓；忽略文件和编辑器未保存的草稿会保留。冲突、子模块和嵌套仓库需单独处理，移到废纸篓失败时不会改为永久删除。
- 原生虚拟化 Diff、语法高亮、行内 / 并排、按文件或区块暂存和取消暂存。区块应用前检查快照，保留 CRLF 与行尾空白。
- Git 提交区默认折叠，保留提交草稿与暂存数量；可展开 Diff 为专注视图，按文件或改动区块前后导航。查看完整文件后可返回原 Diff；近期文件的阅读位置在改动内容不变时保留。
- 可将文件标为“已查看”，标记绑定文件、暂存区和 HEAD 的版本；刷新发现内容变化时取消标记，即使 Git 状态字母相同也能识别。设置检查点会捕获当前磁盘内容（包含 Git 未忽略的未跟踪文件），保留实际暂存区；比较检查点查看下一轮新增改动，支持手动刷新并返回工作区。比较视图不提供暂存操作，检查点和审查标记属于当前运行。
- 多行提交说明、普通提交与 amend；失败保留说明，提交只消费 index。保留本机 hooks 与签名行为。
- 提交图展示父子关系、分叉与合并，并显示分支 / 标签。支持作者 / 说明 / ref / hash 筛选，断线表示被筛选隐藏的提交。历史最多加载 2000 条，滚动和分页时保留轨道；父提交链接可查看尚未加载到列表中的提交。详情提供文件导航，合并提交显示相对第一父提交的改动，完整提交信息可展开查看。
- 创建 / 切换 / 重命名 / 删除本地分支、跟踪远程分支、创建 / 删除标签。
- Worktree 归属同一项目，在 Git 侧栏按折叠列表切换，各目录分别保留终端、标签和草稿。主目录显示独立标记，长分支名不会遮住它。右键 Worktree 可直接选择“移除”，确认框会显示完整目标路径；管理页也保留此入口。支持从新分支、已有分支或游离 HEAD 创建，打开、锁定 / 解锁及清理失效登记。移除保留分支，检查 Git 改动和未保存编辑内容，不提供强制删除。
- 添加 / 删除 remote、更新 URL、fetch、快进 pull、merge / rebase pull、push 和首次设置 upstream。
- Stash 保存（包含未跟踪文件）、预览、apply、pop、drop；操作绑定对象 ID，删除在 Git 引用锁内校验目标。当前支持 files 引用存储；其他存储格式会保留条目并提示终端处理。
- merge / rebase 状态、冲突三阶段内容查看、结果编辑、标记解决、继续和中止；删除文件也可作为解决结果。
- Git 页每 3 秒在后台刷新，切入页面或操作完成时刷新；后台结果保留项目归属并拒绝过期 Diff / 文件读取。

使用现有 Git 配置、SSH、credential helper 和签名程序；需要交互认证时可在终端完成。编辑器限 UTF-8 文本与 4 MiB 文件；非 UTF-8 Diff 给出提示，整文件暂存仍保留原始字节。Git 输出上限 8 MiB。交互式 rebase、PR / Issue / CI 集成与终端进程持久化属于后续工作。

创建终端时会移除启动器继承的 `NO_COLOR`，并声明 `xterm-256color` / `truecolor` 能力，避免从无色命令环境启动应用时让 TUI 自动关闭颜色。用户的 Shell 配置或单条命令仍可显式设置 `NO_COLOR`。

当前终端主要实现标准 xterm/VT 交互；图片协议、完整 Kitty 键盘协议和终端进程持久化尚未实现。兼容性需要按具体插件和程序验证。

## 快捷键

顶部系统菜单栏提供“文件 / 视图 / 终端”：打开文件夹或文件、快速查找与项目搜索、保存与标签管理、切换模块与主题，以及新建、分屏、清空和复制终端输出。菜单随界面语言即时切换，不适用的操作自动禁用；清空和复制输出作用于当前获得焦点的终端窗格。

| macOS | Windows / Linux | 操作 |
| --- | --- | --- |
| ⌘O | Ctrl+Shift+O | 打开文件夹 |
| ⌘B | Ctrl+Shift+B | 展开 / 收起项目栏 |
| ⌘⌥P | Ctrl+Alt+P | 搜索项目与工作目录 |
| ⌘J | Ctrl+Shift+J | 在 Git / 文件页展开或收起底部终端 |
| 终端内 ⌘F | 终端内 Ctrl+Shift+F | 搜索终端输出和滚动历史 |
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
| ⌘⇧C | Ctrl+Shift+C | 在文件源码 / Git Diff 中复制带路径和行号的选区上下文 |
| ⌘K | Ctrl+Shift+K | 清空获得焦点的终端屏幕与滚动历史，并请求程序重绘 |
| Ctrl+K | Ctrl+K | 发送给 Shell / TUI，保留程序原有快捷键 |
| ⇧PageUp / ⇧PageDown | Shift+PageUp / Shift+PageDown | 终端历史翻页 |
| Esc | Esc | 终端焦点下发送给程序；其他区域优先关闭弹层，再关闭窗口 |

## 验证

```sh
python3 -B -m unittest discover -s tests -p 'test_*.py'
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
