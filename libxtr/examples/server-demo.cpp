#include <iostream>
#include <xtr/xtr.h>
// #include <unistd.h>
#include <thread>
#include <chrono>

using namespace std::chrono_literals;

static void on_packet(XtrSessionId const* ssid, XtrPacketRef* packet, void* opaque)
{
    printf("on_packet\n");
    XtrPacketRelease(packet);
}

static void on_state(XtrSessionId const* ssid, XtrSessionState state, void* opaque)
{
    printf("on_state\n");
    char ssid_str[64];
    if (XtrSessionIdToString(ssid, ssid_str, sizeof(ssid_str)) > 0) {
        printf("SessionId=%s\n", ssid_str);
    }
}

int main(int argc, char** argv)
{
    XtrInitialize();
    auto xtr = XtrServerNew("127.0.0.1:6600", 0);
    XtrServerSetPacketCB(xtr, on_packet, nullptr);
    XtrServerSetStateCB(xtr, on_state, nullptr);
    XtrServerStart(xtr);
    // for (int i = 0; i < 100; i++) {
    //     auto pv = XtrPackedValuesNew();
    //     XtrPackedValuesPutU32(pv, 0x0001, 5000);
    //     XtrPackedValuesPutU32(pv, 0x0002, 50);
    //     XtrPackedValuesPutU32(pv, 0x0003, 1);
    //     XtrPackedValuesPutU32(pv, 0x0004, 1);
    //     auto pk = XtrPacketNewPackedValues(pv, 0, 1);
    //     XtrClientPostPacket(xtr, pk);
    //     XtrPackedValuesRelease(pv);
    //     XtrPacketRelease(pk);
    //     std::this_thread::sleep_for(100ms);
    // }
    getchar();
    XtrServerStop(xtr);
    XtrServerRelease(xtr);
    XtrFinalize();
    std::cout << "Hello, Xtr!" << std::endl;
    return 0;
}
