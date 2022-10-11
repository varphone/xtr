#ifndef XTR_XTR_H
#define XTR_XTR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// 一个代表数据包标志的枚举。
enum XtrPacketFlags {
    XTR_PKT_FLAG_END_STREAM = 0x01,
    XTR_PKT_FLAG_END_HEADERS = 0x4,
    XTR_PKT_FLAG_PADDED = 0x8,
    XTR_PKT_FLAG_PRIORITY = 0x20,
    XTR_PKT_FLAG_READONLY = 0x40,
};

/// 一个代表数据包类型的枚举。
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
    XTR_PKT_TYPE_BIN_CODE,
    XTR_PKT_TYPE_JSON,
    XTR_PKT_TYPE_PACKED_VALUES,
    XTR_PKT_TYPE_UNKNOWN,
};

/// 一个代表客户端状态的枚举。
enum XtrClientState {
    XTR_CLI_STAT_CONNECTED,
    XTR_CLI_STAT_CONNECT_ERROR,
    XTR_CLI_STAT_CONNECT_TIMEOUT,
    XTR_CLI_STAT_DISCONNECTED,
    XTR_CLI_STAT_TRY_RECONNECT,
};

/// 一个代表打包值条目的类型。
struct XtrPackedItem {
    uint16_t addr;
    uint8_t kind;
    uint8_t elms;
};

typedef struct XtrClientRef XtrClientRef;
typedef struct XtrPackedValuesRef XtrPackedValuesRef;
typedef struct XtrPacketRef XtrPacketRef;
typedef struct XtrPackedItem XtrPackedItem;
typedef struct XtrPackedItemIterRef XtrPackedItemIterRef;

typedef void (*XtrClientPacketHandler)(XtrPacketRef* packet, void* opaque);
typedef void (*XtrClientStateHandler)(enum XtrClientState state, void* opaque);

//============================================================================
// Xtr
//============================================================================

/// 初始化 XTR 框架。
void XtrInitialize(void);

/// 释放 XTR 框架。
void XtrFinalize(void);

//============================================================================
// XtrClient
//============================================================================

/// 创建客户端实例。
XtrClientRef* XtrClientNew(char const* addr, uint32_t flags);

/// 增加客户端实例引用计数。
XtrClientRef* XtrClientAddRef(XtrClientRef* xtr);

/// 减少客户端实例引用计数，当减到 0 时会释放资源。
void XtrClientRelease(XtrClientRef* xtr);

/// 设置客户端实例数据包回调。
void XtrClientSetPacketCB(XtrClientRef* xtr, XtrClientPacketHandler cb, void* opaque);

/// 设置客户端实例状态回调。
void XtrClientSetStateCB(XtrClientRef* xtr, XtrClientStateHandler cb, void* opaque);

/// 启动客户端实例。
int32_t XtrClientStart(XtrClientRef* xtr);

/// 停止客户端实例。
int32_t XtrClientStop(XtrClientRef* xtr);

/// 向客户端连接推入一个数据包并立即返回。
///
/// 推入的数据包会存放在队列中由后台按顺序发送到连接的远端。
int32_t XtrClientPostPacket(XtrClientRef* xtr, XtrPacketRef* packet);

/// 尝试向客户端连接推入一个数据包并立即返回。
///
/// 推入的数据包会存放在队列中由后台按顺序发送到连接的远端。
int32_t XtrClientTryPostPacket(XtrClientRef* xtr, XtrPacketRef* packet);

/// 向客户端连接发送一个数据包并等待返回。
XtrPacketRef* XtrClientSendPacket(XtrClientRef* xtr, XtrPacketRef* packet);

//============================================================================
// XtrPacket
//============================================================================

XtrPacketRef* XtrPacketNewData(uint32_t length, uint8_t flags, uint32_t stream_id);
XtrPacketRef* XtrPacketNewPackedValues(XtrPackedValuesRef* pv, uint8_t flags, uint32_t stream_id);

XtrPacketRef* XtrPacketAddRef(XtrPacketRef* packet);
void XtrPacketRelease(XtrPacketRef* packet);

uint8_t XtrPacketGetFlags(XtrPacketRef const* packet);
uint32_t XtrPacketGetLength(XtrPacketRef const* packet);
uint32_t XtrPacketGetSequence(XtrPacketRef const* packet);
uint32_t XtrPacketGetStreamId(XtrPacketRef const* packet);
uint64_t XtrPacketGetTimestamp(XtrPacketRef const* packet);
uint8_t XtrPacketGetType(XtrPacketRef const* packet);

uint8_t const* XtrPacketGetConstData(XtrPacketRef const* packet);
uint8_t* XtrPacketGetData(XtrPacketRef* packet);

//============================================================================
// XtrPackedValues
//============================================================================

XtrPackedValuesRef* XtrPackedValuesNew(void);
XtrPackedValuesRef* XtrPackedValuesWithBytes(uint8_t const* data, uint32_t length);
void XtrPackedValuesRelease(XtrPackedValuesRef* pv);

int32_t XtrPackedValuesGetI8(XtrPackedValuesRef* pv, uint16_t addr, int8_t* val);
int32_t XtrPackedValuesGetI16(XtrPackedValuesRef* pv, uint16_t addr, int16_t* val);
int32_t XtrPackedValuesGetI32(XtrPackedValuesRef* pv, uint16_t addr, int32_t* val);
int32_t XtrPackedValuesGetI64(XtrPackedValuesRef* pv, uint16_t addr, int64_t* val);

int32_t XtrPackedValuesGetI8s(XtrPackedValuesRef* pv, uint16_t addr, int8_t* vals, uint16_t num);
int32_t XtrPackedValuesGetI16s(XtrPackedValuesRef* pv, uint16_t addr, int16_t* vals, uint16_t num);
int32_t XtrPackedValuesGetI32s(XtrPackedValuesRef* pv, uint16_t addr, int32_t* vals, uint16_t num);
int32_t XtrPackedValuesGetI64s(XtrPackedValuesRef* pv, uint16_t addr, int64_t* vals, uint16_t num);

int32_t XtrPackedValuesGetU8(XtrPackedValuesRef* pv, uint16_t addr, uint8_t* val);
int32_t XtrPackedValuesGetU16(XtrPackedValuesRef* pv, uint16_t addr, uint16_t* val);
int32_t XtrPackedValuesGetU32(XtrPackedValuesRef* pv, uint16_t addr, uint32_t* val);
int32_t XtrPackedValuesGetU64(XtrPackedValuesRef* pv, uint16_t addr, uint64_t* val);

int32_t XtrPackedValuesGetU8s(XtrPackedValuesRef* pv, uint16_t addr, uint8_t* vals, uint16_t num);
int32_t XtrPackedValuesGetU16s(XtrPackedValuesRef* pv, uint16_t addr, uint16_t* vals, uint16_t num);
int32_t XtrPackedValuesGetU32s(XtrPackedValuesRef* pv, uint16_t addr, uint32_t* vals, uint16_t num);
int32_t XtrPackedValuesGetU64s(XtrPackedValuesRef* pv, uint16_t addr, uint64_t* vals, uint16_t num);

int32_t XtrPackedValuesGetF32(XtrPackedValuesRef* pv, uint16_t addr, float* val);
int32_t XtrPackedValuesGetF64(XtrPackedValuesRef* pv, uint16_t addr, double* val);

int32_t XtrPackedValuesGetF32s(XtrPackedValuesRef* pv, uint16_t addr, float* vals, uint16_t num);
int32_t XtrPackedValuesGetF64s(XtrPackedValuesRef* pv, uint16_t addr, double* vals, uint16_t num);

int32_t XtrPackedValuesPutI8(XtrPackedValuesRef* pv, uint16_t addr, int8_t val);
int32_t XtrPackedValuesPutI16(XtrPackedValuesRef* pv, uint16_t addr, int16_t val);
int32_t XtrPackedValuesPutI32(XtrPackedValuesRef* pv, uint16_t addr, int32_t val);
int32_t XtrPackedValuesPutI64(XtrPackedValuesRef* pv, uint16_t addr, int64_t val);

int32_t XtrPackedValuesPutI8s(XtrPackedValuesRef* pv, uint16_t addr, int8_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutI16s(XtrPackedValuesRef* pv, uint16_t addr, int16_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutI32s(XtrPackedValuesRef* pv, uint16_t addr, int32_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutI64s(XtrPackedValuesRef* pv, uint16_t addr, int64_t const* vals, uint16_t num);

int32_t XtrPackedValuesPutU8(XtrPackedValuesRef* pv, uint16_t addr, uint8_t val);
int32_t XtrPackedValuesPutU16(XtrPackedValuesRef* pv, uint16_t addr, uint16_t val);
int32_t XtrPackedValuesPutU32(XtrPackedValuesRef* pv, uint16_t addr, uint32_t val);
int32_t XtrPackedValuesPutU64(XtrPackedValuesRef* pv, uint16_t addr, uint64_t val);

int32_t XtrPackedValuesPutU8s(XtrPackedValuesRef* pv, uint16_t addr, uint8_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutU16s(XtrPackedValuesRef* pv, uint16_t addr, uint16_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutU32s(XtrPackedValuesRef* pv, uint16_t addr, uint32_t const* vals, uint16_t num);
int32_t XtrPackedValuesPutU64s(XtrPackedValuesRef* pv, uint16_t addr, uint64_t const* vals, uint16_t num);

int32_t XtrPackedValuesPutF32(XtrPackedValuesRef* pv, uint16_t addr, float val);
int32_t XtrPackedValuesPutF64(XtrPackedValuesRef* pv, uint16_t addr, double val);

int32_t XtrPackedValuesPutF32s(XtrPackedValuesRef* pv, uint16_t addr, float const* vals, uint16_t num);
int32_t XtrPackedValuesPutF64s(XtrPackedValuesRef* pv, uint16_t addr, double const* vals, uint16_t num);

XtrPackedItemIterRef* XtrPackedValuesItemIter(XtrPackedValuesRef* pv);
void XtrPackedValuesItemIterRelease(XtrPackedItemIterRef* iter);

XtrPackedItem XtrPackedValuesItemNext(XtrPackedItemIterRef* pv);

#ifdef __cplusplus
}
#endif

#endif // XTR_XTR_H
