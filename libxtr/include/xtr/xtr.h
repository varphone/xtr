#ifndef XTR_XTR_H
#define XTR_XTR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum XtrPacketFlags {
    XTR_PKT_FLAG_END_STREAM = 0x01,
    XTR_PKT_FLAG_END_HEADERS = 0x4,
    XTR_PKT_FLAG_PADDED = 0x8,
    XTR_PKT_FLAG_PRIORITY = 0x20,
};

enum XtrPacketType {
    XTR_PKT_TYPE_DATA = 0,
    XTR_PKT_TYPE_HEADERS,
    XTR_PKT_TYPE_PRIORITY,
    XTR_PKT_TYPE_RESET,
    XTR_PKT_TYPE_SETTINGS,
    XTR_PKT_TYPE_PUSH_PROMISE,
    XTR_PKT_TYPE_PING,
    XTR_PKT_TYPE_GO_AWAY,
    XTR_PKT_TYPE_WINDOW_UPDATE,
    XTR_PKT_TYPE_CONTINUATION,
    XTR_PKT_TYPE_UNKNOWN,
};

enum XtrClientState {
    XTR_CLI_STAT_CONNECTED,
    XTR_CLI_STAT_CONNECT_ERROR,
    XTR_CLI_STAT_CONNECT_TIMEOUT,
    XTR_CLI_STAT_DISCONNECTED,
    XTR_CLI_STAT_TRY_RECONNECT,
};

typedef struct XtrClientRef XtrClientRef;
typedef struct XtrPacketRef XtrPacketRef;

typedef void (*XtrClientPacketHandler)(XtrPacketRef* packet, void* opaque);
typedef void (*XtrClientStateHandler)(enum XtrClientState state, void* opaque);

XtrClientRef* xtr_client_new(char const* addr, uint32_t flags);

void xtr_client_set_packet_cb(XtrClientRef* xtr, XtrClientPacketHandler cb, void* opaque);
void xtr_client_set_state_cb(XtrClientRef* xtr, XtrClientStateHandler cb, void* opaque);

int32_t xtr_client_start(XtrClientRef* xtr);
int32_t xtr_client_stop(XtrClientRef* xtr);

XtrClientRef* xtr_client_ref(XtrClientRef* xtr);
void xtr_client_unref(XtrClientRef* xtr);

XtrPacketRef* xtr_packet_new_data(uint32_t length, uint8_t flags, uint32_t stream_id);

uint8_t xtr_packet_flags(XtrPacketRef const* pkt_ref);
uint32_t xtr_packet_length(XtrPacketRef const* pkt_ref);
uint8_t xtr_packet_type(XtrPacketRef const* pkt_ref);

uint8_t const* xtr_packet_const_data(XtrPacketRef const* pkt_ref);
uint8_t* xtr_packet_data(XtrPacketRef* pkt_ref);

XtrPacketRef* xtr_packet_ref(XtrPacketRef* pkt_ref);
void xtr_packet_unref(XtrPacketRef* pkt_ref);

#ifdef __cplusplus
}
#endif

#endif // XTR_XTR_H
