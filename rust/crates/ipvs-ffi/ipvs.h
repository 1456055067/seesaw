/* C header for Rust IPVS FFI
 *
 * This file defines the C interface to the Rust IPVS implementation.
 * Use this header when calling from Go via CGo.
 */

#ifndef IPVS_H
#define IPVS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to IPVS manager */
typedef struct IpvsHandle IpvsHandle;

/* C-compatible service structure */
typedef struct {
    uint32_t address;      /* IPv4 address in network byte order */
    uint8_t protocol;      /* TCP=6, UDP=17, SCTP=132 */
    uint16_t port;         /* Port in network byte order */
    uint32_t fwmark;       /* Firewall mark (0 if not used) */
    const char* scheduler; /* Scheduler name (null-terminated) */
    uint32_t flags;        /* Service flags */
    uint32_t timeout;      /* Connection timeout */
} CService;

/* C-compatible destination structure */
typedef struct {
    uint32_t address;          /* IPv4 address in network byte order */
    uint16_t port;             /* Port in network byte order */
    uint32_t weight;           /* Weight for load balancing */
    uint8_t fwd_method;        /* Forwarding method: 0=Masq, 1=Local, 2=Tunnel, 3=Route, 4=Bypass */
    uint32_t lower_threshold;  /* Lower connection threshold */
    uint32_t upper_threshold;  /* Upper connection threshold */
} CDestination;

/* C-compatible version structure */
typedef struct {
    uint32_t major;
    uint32_t minor;
    uint32_t patch;
} CVersion;

/* Error codes */
enum IpvsError {
    IPVS_SUCCESS = 0,
    IPVS_NULL_POINTER = -1,
    IPVS_INVALID_UTF8 = -2,
    IPVS_ERROR = -3,
    IPVS_NETLINK_ERROR = -4,
    IPVS_UNKNOWN = -99
};

/* Create a new IPVS manager instance */
IpvsHandle* ipvs_new(void);

/* Destroy an IPVS manager instance */
void ipvs_destroy(IpvsHandle* handle);

/* Get IPVS kernel version */
int ipvs_version(IpvsHandle* handle, CVersion* version);

/* Flush all IPVS services */
int ipvs_flush(IpvsHandle* handle);

/* Service operations */
int ipvs_add_service(IpvsHandle* handle, const CService* service);
int ipvs_update_service(IpvsHandle* handle, const CService* service);
int ipvs_delete_service(IpvsHandle* handle, const CService* service);

/* Destination operations */
int ipvs_add_destination(IpvsHandle* handle, const CService* service, const CDestination* dest);
int ipvs_update_destination(IpvsHandle* handle, const CService* service, const CDestination* dest);
int ipvs_delete_destination(IpvsHandle* handle, const CService* service, const CDestination* dest);

/* Get error string for error code */
const char* ipvs_error_string(int error_code);

#ifdef __cplusplus
}
#endif

#endif /* IPVS_H */
