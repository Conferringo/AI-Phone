# Simulated location

The toolbar entry immediately before Knowledge controls the managed Android phone and the Linux desktop's built-in Chromium browser. It is off by default. Opening the panel while disabled does not request OS geolocation permission, load map tiles or perform network geolocation.

Enabling uses the server host's outbound network location as an approximation, not the remote browser's location and not GPS precision. Select a map point, drag the marker or edit WGS84 latitude/longitude for precision. Network lookup failure leaves manual selection available. The external IP-location service uses its own JSON format; AI Phone's business API and guest command/receipt envelopes use protobuf.

Route mode adds ordered waypoints by clicking the map; click a waypoint to remove it or use Undo. Speed is 0.5–500 km/h. Motion follows straight segments by geographic distance, supports once/repeat and pauses on unknown/failed device outcomes. No road-routing service is implied. Process restarts require deliberate motion resumption. Disabling removes AI Phone's Android mock providers and Chromium overrides; apps can retain their own last-position cache.

Android uses the same owner/device/run/control-epoch/action-id receipt ledger as phone-use. Updating location does not invalidate screen targets or move the touch cursor. Linux support currently covers the built-in Chromium browser; other desktop applications and browsers need their own adapters. Website permission remains controlled inside the guest browser. Position simulation does not change public IP address, Wi-Fi, radio or account/device verification state.

The local database stores configuration and receipts as private user data. No coordinate, route, map screenshot or state is included in source releases. OpenStreetMap tiles reveal the requested map area to their provider when enabled. Optional host lookup uses ipwho.is and reveals the host's network egress address to that provider. Provider failures are shown; the UI never reports an unsupported device as synchronized.

Validated on the existing managed Android and Linux guests: static fix, route movement, pause, and removal. Fresh-host installation and other Linux applications are not validated. See the source of truth in the local development tracker for detailed private receipts.

## 中文

模拟位置入口在知识库左侧，默认关闭。关闭时打开窗口不会请求系统定位权限、网络定位或地图瓦片。开启后以服务端宿主的出口网络位置作近似值，再使用地图选点、拖动或经纬度精调；网络定位失败也能手动选点。

路线采用 WGS84 坐标之间的直线段，支持速度、单次/循环、暂停；不是道路导航。失败或结果未知时暂停，重启后需手动恢复。手机使用现有操作账本，关闭时移除本功能的模拟定位源；电脑当前覆盖内置 Chromium，其他 Linux 应用尚未接通。不会修改公网 IP、无线网络、通信基站或账号验证状态。网站的定位权限仍由客体浏览器控制。

配置和路线只保存在本地用户数据库，公开源码不包含坐标。启用后地图服务可看到所请求区域，网络定位服务可看到宿主出口地址。已有真实客体的静态定位、路线移动、暂停和关闭已验证；不宣称全新安装或所有 Linux 应用已通过。
