#include <chrono>
#include <iostream>
#include <thread>
#include <xtr/xtr.h>

using namespace std::chrono_literals;

static void on_packet(XtrPacketRef* packet, void* opaque)
{
    printf("on_packet\n");
    XtrPacketRelease(packet);
}

static void on_state(XtrClientState state, void* opaque)
{
    printf("on_state\n");
}

int main(int argc, char** argv)
{
    XtrInitialize();
    auto xtr = XtrClientNew("192.168.2.161:6600", 0);
    XtrClientSetPacketCB(xtr, on_packet, nullptr);
    XtrClientSetStateCB(xtr, on_state, nullptr);
    XtrClientStart(xtr);
    for (int i = 0; i < 100; i++) {
        auto pv = XtrPackedValuesNew();
        XtrPackedValuesPutU32(pv, 0x0001, 5000);
        XtrPackedValuesPutU32(pv, 0x0002, 50);
        XtrPackedValuesPutU32(pv, 0x0003, 1);
        XtrPackedValuesPutU32(pv, 0x0004, 1);
        auto pk = XtrPacketNewPackedValues(pv, 0, 1);
        XtrClientPostPacket(xtr, pk);
        XtrPackedValuesRelease(pv);
        XtrPacketRelease(pk);
        std::this_thread::sleep_for(100ms);
    }
    getchar();
    XtrClientStop(xtr);
    XtrClientRelease(xtr);
    XtrFinalize();
    std::cout << "Hello, Xtr!" << std::endl;
    return 0;
}
