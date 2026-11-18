# RoboMaster Studio 0.3.0 · Android 手机控制台

电脑上的 API 版和 Codex 版都可以与安卓手机配对。在手机上左右滑动切换需求、检索、原型、验收和配合五个阶段，给当前阶段发消息、回答待确认的问题、查看规格与迭代记录。模型配置与 SolidWorks 工作区保存在电脑上。

## 安装

| 设备 | Release 附件 | 启动方式 |
| --- | --- | --- |
| Windows x64，使用模型 API | `RoboMaster-Studio-API-v0.3.0-win-x64.zip` | 解压后运行 `Start-API-Studio.cmd` |
| Windows x64，使用 Codex | `RoboMaster-Studio-Codex-v0.3.0-win-x64.zip` | 解压后运行 `Start-Codex-Studio.cmd` |
| Android 8 或更高版本 | `RoboMaster-Console-v0.3.0.apk` | 安装后扫描桌面配对码，或从相册识别 |

手机和电脑加入同一 Tailscale 私有网络后，手机可用移动数据连接。电脑在「设置 → 手机控制台」生成两分钟有效的一次性配对码，并为该设备批准「操作当前项目」或「仅查看」权限。[手机连接教学](https://github.com/haku-nomisugiru/autorobomaster/blob/studio-v0.3.0/README.md#手机连接教学)

## 断线与恢复

手机丢失提交回执后会沿用原操作编号重试。移动数据真机测试分别截断了一条消息和一次点选的成功回执，两项各到达电脑两次，各只处理一次。重开电脑和手机应用后，原配对恢复，手机重新显示了最终回答。外网测试使用已登录的 Codex，API 版使用本地模拟模型验证了断线和重启。

若电脑在 CAD 操作执行途中意外退出，桌面会标记「结果未知」。请先查看实际模型，再决定是否重新提交。[真机验收记录](https://github.com/haku-nomisugiru/autorobomaster/blob/studio-v0.3.0/studio/docs/validation/mobile-v0.3.0.md)

本次手机控制功能适用于两个 Windows 桌面入口。DSH 用户可继续使用 [0.2.0 插件](https://github.com/haku-nomisugiru/autorobomaster/releases/download/studio-v0.2.0/RoboMaster-Studio-DSH-v0.2.0.zip)。`SHA256SUMS.txt` 提供本次三个安装包的校验值。
