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

    for (auto i = 0; i < 1000; i++) {
        XtrInitialize();
        auto xtr = XtrClientNew("192.168.2.127:6600", 0);
        std::cout << "xtr" << xtr << " " << i << std::endl;
        XtrClientStart(xtr);
        XtrClientStop(xtr);
        XtrClientRelease(xtr);
        XtrFinalize();
    }

    std::cout << "Hello, Xtr!" << std::endl;


    return 0;
}
