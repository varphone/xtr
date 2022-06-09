#include <iostream>
#include <xtr/xtr.h>
// #include <unistd.h>
#include <thread>
#include <chrono>

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
    printf("AAAAAAAAAAA\n");
    auto xtr = XtrClientNew("127.0.0.1:6600", 0);
    printf("AAAAAAAAAAA 1\n");
    XtrClientSetPacketCB(xtr, on_packet, nullptr);
    printf("AAAAAAAAAAA 2\n");
    XtrClientSetStateCB(xtr, on_state, nullptr);
    printf("AAAAAAAAAAA 3\n");
    XtrClientStart(xtr);
    printf("AAAAAAAAAAA 4\n");
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
