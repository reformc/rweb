http穿透代理
使用场景：
1设备有web管理页面，2设备在NAT网络内部，3设备无法对外暴露端口，4需要同时管理许多个这种设备或连接设备所处的网络。

说明：
cert-key-file:创建ssl密钥。编译rwebs和rwebc时需要密钥文件，如果密钥文件不存在则会编译报错。也可修改相关代码使用ca签发的证书。
rwebs:中转服务器，需要暴露一个udp端口供设备连接要一个tcp端口供浏览器或proxy-change-header连接。
rwebc:编译为so、dll供其他语言调用或者使用rust直接引用此库，调用函数quic_client_run会启动一个连接rwebs服务器的任务，此函数同步运行，直到出错时返回错误代码。
rwebc-demo:一个使用rust调用rwebc编译好的dll的示例。
proxy-change-header:如果需要使用设备的http_proxy功能，需要运行此程序在本地启动一个http_proxy端口，运行参数为rwebs上对应设备的域名地址。

使用方法：
首先使用cargo run --bin cert-key-file生成ssl密钥，编译rwebc和rwebs需要密钥文件存在才能编译。
在设备上调用rwebc，在中心服务器运行rwebs，管理员通过访问中心服务器来访问设备。
中心服务器需要暴露一个tcp端口(供管理员访问)和一个udp端口(供设备连接)，给中心服务器设置一个通配符域名解析。
设备和中心服务器的连接使用quic，quic自带加密，无需另外加密。
管理员和中心服务器的连接使用普通的http连接，通配符为设备标签，可以使用nginx或者其他代理(http_proxy无法使用nginx代理)。
例如设置了通配符域名解析*.example.com到中心服务器，设备a标签为aabbccddeeff。那么可以通过访问aabbccddeeff.example.com来访问设备a的web页面或者使用proxy-change-header来使用设备所处的网络

原理：
设备运行rwebc后，会发送自己的mac(或自定义标签)到中心服务器注册，例如设备标签为aabbccddeeff。
在tcp层做代理，所以websocket、sse等协议都支持，暂不支持设备管理页面为https的连接。

示例：
1 cargo run --bin cert-key-file
2 在服务器上运行cargo run --bin rwebs -- --port=5677，同时服务器打开5677的tcp和udp端口，同时设置一个通配符域名解析到服务器，例如设置*.abc.com到服务器
3 在无法暴露公网ip和端口的设备上运行cargo run --bin rwebc-demo -- --server_host=server.abc.com --server_port=5677 --proxy_addr=127.0.0.1:80 --mac=aabbccddeeff
    proxy_addr必须带端口，因为使用的是tcp层代理，mac为设备标签，可以使用mac地址，每个设备的mac必须唯一，不可重复。
4.1 如果不需要使用http_proxy，那么可以在任何地方使用浏览器打开aabbccddeeff.abc.com即可访问aabbccddeeff这台设备上的127.0.0.1:80了
4.2 如果需要使用http_proxy,那么在自己电脑上运行cargo run --bin proxy-change-header -- -p=5678 -d=aabbccddeeff.abc.com:5677,然后设置xshell或者浏览器的代理为http代理，代理地址为127.0.0.1:5678,那么就可以使用设备来代理上网了。可以访问设备所处网络里的网络