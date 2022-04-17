#include <iostream>
#include <xtr/xtr.h>

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
    auto xtr = XtrClientNew("127.0.0.1:9900", 0);
    XtrClientSetPacketCB(xtr, on_packet, nullptr);
    XtrClientSetStateCB(xtr, on_state, nullptr);
    XtrClientStart(xtr);
    getchar();
    XtrClientStop(xtr);
    XtrClientRelease(xtr);
    std::cout << "Hello, Xtr!" << std::endl;
    return 0;
}
