/** Fixed installation instructions appended to every generated changelog. */
export function installationNotes(language) {
  if (language === 'en') {
    return `### Installation

- **macOS**: Use \`ossia-darwin-arm64.dmg\` on Apple Silicon or \`ossia-darwin-amd64.dmg\` on Intel.
- **Windows**: Use \`ossia-windows-amd64.exe\` on 64-bit Windows 10 or later.
- **Linux**: Use \`ossia-linux-amd64.deb\` on Debian/Ubuntu or \`ossia-linux-amd64.rpm\` on Fedora/RHEL.
- **Debian/Ubuntu**: \`sudo apt install ./ossia-linux-amd64.deb\`
- **Fedora/RHEL**: \`sudo dnf install ./ossia-linux-amd64.rpm\`

The app bundles LilyPond, Python, Guile and fonts. Linux still needs the system WebKitGTK/runtime libraries installed by the package manager.

> These test packages are unsigned. macOS may show a Gatekeeper prompt; choose **System Settings → Privacy & Security → Open Anyway**. Windows may show SmartScreen; choose **More info → Run anyway**.

The complete source archive and third-party materials are attached to this release. Verify downloads against \`SHA256SUMS\`.
`;
  }
  if (language === 'zh') {
    return `### 安装说明

- **macOS**：Apple Silicon 使用 \`ossia-darwin-arm64.dmg\`，Intel 使用 \`ossia-darwin-amd64.dmg\`。
- **Windows**：Windows 10 及以上 64 位系统使用 \`ossia-windows-amd64.exe\`。
- **Linux**：Debian/Ubuntu 使用 \`ossia-linux-amd64.deb\`，Fedora/RHEL 使用 \`ossia-linux-amd64.rpm\`。
- **Debian/Ubuntu**：\`sudo apt install ./ossia-linux-amd64.deb\`
- **Fedora/RHEL**：\`sudo dnf install ./ossia-linux-amd64.rpm\`

应用已内置 LilyPond、Python、Guile 和字体；Linux 仍需要包管理器安装的系统 WebKitGTK／运行时库。

> 当前为未签名测试包。macOS 如出现安全提示，前往 **系统设置 → 隐私与安全性 → 仍要打开**；Windows 如出现 SmartScreen，选择 **更多信息 → 仍要运行**。

Release 同时附带完整源码包和第三方材料。下载后请使用 \`SHA256SUMS\` 校验。
`;
  }
  throw new Error(`unsupported release notes language ${language}`);
}
