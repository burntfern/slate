# Changelog

## 0.1.0 (2026-10-17)


### Features

* **app:** add keyboard shortcuts, ctrl-drag tabs, and a persistent config store ([3a5aa05](https://github.com/burntfern/slate/commit/3a5aa054d628c603f7cd7fcb21c774f6139691da))
* **controller:** drive tab groups and host/member sync ([6c34bd2](https://github.com/burntfern/slate/commit/6c34bd28d8c05b0a173e41b80eb6a9b55d829ceb))
* **core:** add group state machine and workspace restore planner with tests ([46f1fda](https://github.com/burntfern/slate/commit/46f1fdab0c820721fc50fed857e230fc13218dd2))
* **tabs:** scroll overflowing tab strip and drag tabs out to detach ([1936504](https://github.com/burntfern/slate/commit/1936504408c0f34891e03680b713f379e5c18c76))
* **tray:** add system tray icon and drop app rules ([81a994c](https://github.com/burntfern/slate/commit/81a994c31981b5c571a3ef642aa8373f36c99ec3))
* **ui:** add per-tab accent colors ([315173f](https://github.com/burntfern/slate/commit/315173f0619646928d6928c87b54ad9578853691))
* **ui:** add snapshot-based tab hover preview ([86204f5](https://github.com/burntfern/slate/commit/86204f50b18ef763908be9dae116f0533324a3de))
* **ui:** add tab context menu, window picker, and ungroup/close ([6fa1110](https://github.com/burntfern/slate/commit/6fa1110d7e6964618fc15925940847436af2d748))
* **ui:** add tabbed host window shell ([db8e5e3](https://github.com/burntfern/slate/commit/db8e5e32307d92625f5d095ad14a6fb2f6bf3280))
* **update:** add update checks, version menu, and keycap empty states ([72aa6a0](https://github.com/burntfern/slate/commit/72aa6a06179df2f3d8132844ea5671bedea99863))
* **win:** add window enumeration, elevation detection, and foreground activation ([f88799c](https://github.com/burntfern/slate/commit/f88799cf1fc4f7245dd6fa5898aa8fa926e7f69c))
* **win:** add WinEvent hooks for window destroy, title, and foreground ([bffbe6a](https://github.com/burntfern/slate/commit/bffbe6a235a0ba5989e77554a7579974a3f096b4))
* **win:** host foreign windows via reparent and region crop ([83eccda](https://github.com/burntfern/slate/commit/83eccda3216ee4501f46ea37746c62d5765bccd5))


### Bug Fixes

* **persistence:** survive corrupt config and janitor state files ([9583e41](https://github.com/burntfern/slate/commit/9583e41a1e07ef6b63e5fd2d11a784e414d80453))
* **win:** stop the tab crashes and Win32 handle leaks ([a385eaf](https://github.com/burntfern/slate/commit/a385eaf7f5acd94478775d307e0afbab1579b869))


### Refactoring

* **all:** split modules and trim render work ([31fa925](https://github.com/burntfern/slate/commit/31fa925f010fc6d253b89c9e28c76bdc1d3ef7b1))
* **app:** apply audit fixes, a11y roles, CI hardening and bug fixes ([31b8019](https://github.com/burntfern/slate/commit/31b80198d3a29af379320ae80d6dd0ada815be76))
* **controller:** split the module into focused submodules ([391d7e1](https://github.com/burntfern/slate/commit/391d7e17b0fc200f97e1c63d68b3cce3eb4ca1eb))
* **core:** split the core and config modules ([120fb09](https://github.com/burntfern/slate/commit/120fb094e84791faf176d7fe98adf201df74c1af))


### Documentation

* **readme:** restructure the README and refresh the gallery captures [skip ci] ([30c8835](https://github.com/burntfern/slate/commit/30c883505a3451b9bd1f5587e319b99195207642))
* **repo:** add the readme, contributing guide, and license ([3ef90ab](https://github.com/burntfern/slate/commit/3ef90abf5cef4d9e3279968d5a8cd4bc936b9c5e))
* **repo:** add the security policy and project metadata ([e57e514](https://github.com/burntfern/slate/commit/e57e514b98b336ed5f98e29f99bf83542af69adb))
