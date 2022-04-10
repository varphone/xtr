xtr
===

高可扩展性传输协议框架。

C/C++ 示例
----------

```cpp
#include <xtr/xtr.h>
#include <chrono>
#include <iostream>
#include <thread>

using namespace std::chrono_literals;

static void on_packet(XtrPacketRef* packet, void* opaque)
{
    std::cout << "on_packet\n";
    XtrPacketRelease(packet);
}

static void on_state(XtrClientState state, void* opaque)
{
    std::cout << "on_state\n";
}

int main(int argc, char** argv)
{
    // 创建客户端实例
    auto xtr = XtrClientNew("192.168.2.161:6600", 0);
    XtrClientSetPacketCB(xtr, on_packet, nullptr);
    XtrClientSetStateCB(xtr, on_state, nullptr);
    XtrClientStart(xtr);

    for (int i = 0; i < 100; i++) {
        // 打包参数值
        auto pv = XtrPackedValuesNew();
        XtrPackedValuesPutU32(pv, 0x0001, 5000);
        XtrPackedValuesPutU32(pv, 0x0002, 50);
        XtrPackedValuesPutU32(pv, 0x0003, 1);
        XtrPackedValuesPutU32(pv, 0x0004, 1);
        // 创建要发送的数据包
        auto pk = XtrPacketNewPackedValues(pv, 0, 1);
        XtrClientPostPacket(xtr, pk);
        XtrPackedValuesRelease(pv);
        XtrPacketRelease(pk);
        //
        std::this_thread::sleep_for(100ms);
    }

    getchar();

    // 停止及销毁客户端实例
    XtrClientStop(xtr);
    XtrClientRelease(xtr);

    return 0;
}

```
