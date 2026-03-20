## Disclaimer

This project is intended for personal and educational use only. Use of automation or botting tools may violate Nexon's Terms of Service and can result in penalties, including permanent account bans. I do not guarantee continued development or maintenance of this project. It may be discontinued at any time. This project is not affiliated with or endorsed by Nexon. Use at your own risk.

## Building the bot (Windows only)

Ensure the following tools and dependencies are installed:
- Rustup
- [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/#install-the-dioxus-cli)
- LLVM 21.x
- NodeJS
- OpenCV 4 statically linked binaries via `vcpkg install opencv4[contrib,nonfree]:x64-windows-static`

Then build with the following steps:
1. Inside your `%USERPROFILE%/.cargo/config.toml`, add:
```toml
[env]
OPENCV_DISABLE_PROBES = "environment,pkg_config,cmake,vcpkg_cmake"
VCPKGRS_TRIPLET = "x64-windows-static"
VCPKG_ROOT = # Your vcpkg folder
```
2. If it is your first time building, run `npm install` inside `ui` directory
3. Run `dx build --release --package ui` in the root directory

## Support this project

If you wish to support this project, thank you! I currently accept donations via cryptocurrency:

- Ethereum (ETH, USDC, USDT): 0x8EE660010a3f2D65b525F6b51975A4BEC82e1F09
- Solana (SOL): ADM9b1gxaR5FA9KeCSV2NMN8XyjcgTKcJbqaWoQXk4wV
