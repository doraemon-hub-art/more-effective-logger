# 配置文件说明

## 概述

应用会自动在 `~/.config/melogger/config.toml` 保存 SSH 会话信息。

## 配置文件位置

```
~/.config/melogger/config.toml
```

## 配置文件格式

```toml
[session]
host = "192.168.1.98:22"
username = "root"
password = "your_password"
```

## 使用方式

1. 在 Session 界面输入 SSH 连接信息（Host、Username、Password）
2. 点击 "Connect" 按钮
3. 连接成功后，配置会自动保存到 `~/.config/melogger/config.toml`
4. 下次打开应用时，Session 界面会自动填充上次保存的信息

## 注意事项

- 密码以明文形式保存在配置文件中，请确保文件权限安全
- 配置文件目录会自动创建，无需手动创建
- 目前只支持保存一个会话信息
