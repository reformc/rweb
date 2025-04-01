http穿透代理
使用场景：
1设备有web管理页面，2设备在NAT网络内部，3设备无法对外暴露端口，4需要同时管理许多个这种设备。

使用方法：
在设备上运行rwebc，在中心服务器运行rwebs，管理员通过访问中心服务器来访问设备。
rwebc可以编译为dll或so供其他程序调用。rwebc-demo为调用demo
中心服务器需要暴露一个tcp端口(供管理员访问)和一个udp端口(供设备连接)，给中心服务器设置一个通配符域名解析。
设备和中心服务器的连接使用quic，quic自带加密，无需另外加密。
管理员和中心服务器的连接使用普通的http连接，通配符为设备标签，可以使用nginx或者其他代理。
例如设置了通配符域名解析*.example.com到中心服务器，设备a标签为aabbccddeeff。那么可以通过访问aabbccddeeff.example.com来访问设备a的web页面

原理：
设备运行rwebc后，会发送自己的mac(或自定义标签)到中心服务器注册，例如设备标签为aabbccddeeff。
在tcp层做代理，所以websocket、sse等协议都支持，暂不支持设备管理页面为https的连接。