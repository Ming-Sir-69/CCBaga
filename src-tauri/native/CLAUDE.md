# native/
> L2 | 父级: ../CLAUDE.md
BrowserTests.m: 虚构参数验证精确资料、相似前缀、重复参数与分隔符边界，并从本测试进程用 sysctl 读取实际 argv；虚构窗口元数据核对准确 PID 与普通窗口范围；不启动/关闭用户浏览器。
清理、剪贴板与电脑设置分别使用独立系统桥；无选区捕获或模拟键盘事务。
IdentityTests.m: 临时独立钥匙串与虚构字段，编译期替换默认钥匙串获取；实际 Security API 验证服务范围、MCP 保留、陈旧拒绝与单项恢复，不改变用户默认钥匙串。
Identity.m: 经用户确认读取默认 Code/Desktop 凭据到原生保险库；系统授权、写前复核正文、单条删除与恢复，不经命令行/UI/网络。
Keychain.m: 用户触发登录钥匙串解锁，不读取翻译 Key，不禁用清理的系统授权弹窗。
CleanupTests.m: 独立临时钥匙串复现外部应用所有者 -25244，并验证原生删除、同服务账号隔离与非 Claude 凭据保留，不触碰登录钥匙串。
Cleanup.m: libproc 按外层 Claude 应用标识识别全部辅助进程；sysctl 仅识别 Node/Bun 首个官方 Claude CLI 脚本参数，不输出完整命令；已知凭据服务；只读属性核验后将单条持久引用解析为系统项目引用；文件钥匙串原生单项删除兼容共享密码，其他实现使用单项查询删除，保留系统错误代码和原因，不读取密码或保留撤回副本。
System.m: Foundation 首选语言、地区格式和时区、CoreLocation 状态、系统授权及稳定标识/语义标签定位开关；定位关闭结果必须经系统读取确认。
Browser.m: 真实机器状态与代理，NSLocale 读取包含 rg 覆盖的有效国家并保留原始区域标识；LaunchServices 注册独立浏览器、按包标识查询运行、读取 http/https 默认处理程序并以 LSSetDefaultHandlerForURLScheme/LSSetDefaultRoleHandlerForContentType 请求设为默认（系统弹窗确认）；资料锁 PID 还须当前用户、包标识、Chromium/CCBrowser 可执行名、唯一且精确匹配的 user-data-dir 及进程创建时间复核，避免陈旧锁误指另一环境；参数不可读保持未知错误，不视为可清理。打开已有窗口先按同一资料归属核验 PID，再只读窗口元数据确认普通窗口；macOS 14+ 在主线程协调让渡激活，旧系统兼容激活，不开重复页面，不需要 AX 权限。

System.m: Foundation 首选语言、地区格式和时区；地区参数先按系统目录规范化做只读预检，再修改时区与偏好并读回，保留后备语言；CoreLocation 与系统授权继续经实际状态核验。
SystemTests.m: 真实 Foundation 地区目录与内存偏好验证 13 项预设、中文等价标识、零写入预检、后备语言保留和同步失败；不修改用户偏好、时区或定位。
Browser.m: 真实机器/代理与默认浏览器系统确认桥；资料占用须同时核对用户、应用、唯一绝对 user-data-dir、目录设备/inode 与进程启动时间；无效 PID、缺失应用字段、异常锁或读取不完整均保持未知，只退出已证明属于请求资料的进程。
BrowserProfileTests.m: 虚构 argv/进程记录与临时目录验证路径别名、独立资料、PID 复用/溢出/非 ASCII、缺失应用字段和异常锁拒绝；真实进程读取仅限测试自身与其已回收子进程，不接触用户浏览器或输出参数。
Clipboard.m: 主线程文本复制、辅助功能设置入口与共享响应释放；不捕获选区、不模拟键盘。
[PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md

Brave 原生进程接口独立校验 com.brave.Browser、/Applications/Brave Browser.app、可执行名、当前用户、精确 user-data-dir 和创建时间；不扩大原 CCBaga 进程匹配范围，不访问登录内容。

资料生命周期观测：按当前用户与唯一精确 user-data-dir 统计已知浏览器主/辅助进程，并复核创建时间；未完整读回返回未知。窗口层仅说明当前有无可见普通窗口（最小化/其他 Space 不等于已关闭）；退出仍为精确资料的正常 terminate，不按名称强杀。

BrowserDiscovery.m: NSWorkspace 默认/HTTPS 注册程序加已知品牌渠道应用包发现；安装与遗留数据分开。只读 Info.plist 和标准 Chromium Local State 的 profile.info_cache 名称，目录回退只查元数据；不读 Cookie/密码/钥匙串，不访问 Safari 数据，不启动应用。4MiB 文件限额、链接/坏格式/无权限返回未知或受限；BrowserDiscoveryTests.m 使用独占夹具验证默认、缺安装有资料、多 Profile、坏注册和禁用/权限分支。browser2zen MIT 稳定路径表署名与许可放 ../third-party。

上游收敛基础：Browser.m 的主进程归属采用共享三态检查，唯一绝对 user-data-dir 指向的目录须与目标 dev/inode 一致；别名可识别，参数/目录/进程不可读保持未知，仅 ESRCH 确认进程不存在。BrowserProfileTests.m 用自建目录、自身 argv 与自建子进程验证，不读取真实浏览器资料。Brave 分派、窗口和辅助进程观测仍为下游增量。
