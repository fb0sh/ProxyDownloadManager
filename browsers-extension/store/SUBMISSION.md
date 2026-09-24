# Edge Add-ons 提交作业单

面向 ProxyDownloadManager 的浏览器扩展。所有文案在本目录，逐字段对位，照抄即可。

- 目标商店：Microsoft Edge 加载项（Partner Center）
- 包：`browsers-extension/dist/proxydm-edge-v0.13.2.zip`
- 本地重打包：`pnpm build:ext && cd browsers-extension/edge && zip -r -X ../dist/proxydm-edge-vX.Y.Z.zip . -x '.*'`
- 配套文档：[listing-zh-CN.md](listing-zh-CN.md) · [listing-en-US.md](listing-en-US.md) · [privacy-declarations.md](privacy-declarations.md) · [certification-notes.md](certification-notes.md)

---

## 0. 只能由你完成的动作

| 动作 | 说明 |
| --- | --- |
| 注册 Partner Center 开发者账号 | https://partner.microsoft.com/dashboard/microsoftedge/public/login ，需要 Microsoft 帐户 (MSA)；**注册免费**。个人账号验证较快，注册后**国家/地区与账号类型不可更改**。 |
| 接受开发人员协议 | 注册表单里的复选框。 |
| 勾选数据使用披露 + "我证明以下披露是真实的" | 建议见 privacy-declarations.md 第 4 节。 |
| 点最后的 **发布 / Publish** | 提交后进入认证，最长约 7 个工作日。 |

---

## 1. 创建扩展 → 上传包

Partner Center → Microsoft Edge → **创建新扩展**，把 `proxydm-edge-v0.13.2.zip` 拖进去。
包内结构：`manifest.json` 直接位于根目录，无外层文件夹（Partner Center 要求如此）。
若校验报错，先本地重新打包再上传。

---

## 2. 可用性 · Availability

| 字段 | 值 |
| --- | --- |
| 可见性 Visibility | Public（默认） |
| 市场 Markets | 所有市场（默认） |

---

## 3. 属性 · Properties

| 字段 | 值 |
| --- | --- |
| 类别 Category | **生产力 / Productivity** |
| 网站 Website | `https://fb0sh.github.io/ProxyDownloadManager/` |
| 支持联系人 Support contact | `https://github.com/fb0sh/ProxyDownloadManager/issues` |
| 成人内容 Adult content | 否 |
| 隐私策略要求（旧 UI） | **是** |
| 隐私策略 URL | `https://fb0sh.github.io/ProxyDownloadManager/privacy.html` |

---

## 4. 隐私 · Privacy

五个区块的完整文案（英文可直贴 + 中文备查）见 **[privacy-declarations.md](privacy-declarations.md)**：

1. 单一用途 Single purpose
2. 权限理由 Permission justifications —— 7 个权限 + `<all_urls>` 各一段
3. 远程代码 Remote code —— 选 **否**
4. 数据使用 Data usage —— 建议勾法 + 理由（**需你确认**）
5. 隐私政策 URL —— 同上

---

## 5. Microsoft Store 一览 · Store listing

语言：**English (United States) + 中文(简体)**（包内 `_locales/en` 与 `_locales/zh_CN` 已就位，
Partner Center 会识别出两种语言）。

| 字段 | 来源 |
| --- | --- |
| 扩展名称 | manifest 只读（`__MSG_extensionName__`） |
| 简短说明 | manifest 只读（`__MSG_extensionDescription__`） |
| 说明 Description | 见 [listing-en-US.md](listing-en-US.md) / [listing-zh-CN.md](listing-zh-CN.md) |
| 扩展徽标 | `store/assets/logo-300x300.png`（300×300，1:1；上传后用"复制"同步到另一语言） |
| 屏幕截图 | 待生成（可选，最多 6 张，必须 1280×800 或 640×480） |
| 搜索词 | 见两个 listing 文件的末尾 |
| YouTube 视频 | 无 |

---

## 6. 认证说明 · Certification notes

提交前最后一页 **认证说明** 填 **[certification-notes.md](certification-notes.md)** 的英文部分。
重点：审核机大概率没装桌面端，必须让测试者知道"离线态是预期行为"，并给出安装包地址。

---

## 7. 状态

| 项 | 状态 |
| --- | --- |
| 打包 `.zip` | ✅ 已完成 |
| `_locales` 双语 + `default_locale` | ✅ 已完成（shared/ + 三个 manifest + build.sh） |
| 徽标 300×300 | ✅ 已完成 |
| 商店文案（en + zh） | ✅ 已完成 |
| 权限理由 / 单一用途 / 远程代码 | ✅ 已完成 |
| 认证说明 | ✅ 已完成 |
| 隐私政策页面 | ✅ 已写入 `public/privacy.html`，随 Pages 工作流发布 |
| 屏幕截图 | ⬜ 待生成 |
| 开发者账号 | ⬜ 待注册 |
| 提交 | ⬜ 待提交 |
