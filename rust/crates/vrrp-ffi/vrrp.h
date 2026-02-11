#ifndef VRRP_FFI_H
#define VRRP_FFI_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VrrpHandle VrrpHandle;

typedef enum CVrrpState {
    VRRP_STATE_INIT = 0,
    VRRP_STATE_BACKUP = 1,
    VRRP_STATE_MASTER = 2,
} CVrrpState;

typedef struct CVrrpConfig {
    uint8_t vrid;
    uint8_t priority;
    uint16_t advert_interval;
    bool preempt;
    const char *_interface;
    const char *primary_ip;
    const char **virtual_ips;
    size_t virtual_ip_count;
} CVrrpConfig;

typedef struct CVrrpStats {
    uint64_t master_transitions;
    uint64_t backup_transitions;
    uint64_t adverts_sent;
    uint64_t adverts_received;
    uint64_t invalid_adverts;
    uint64_t priority_zero_received;
    uint64_t checksum_errors;
} CVrrpStats;

VrrpHandle *vrrp_new(const CVrrpConfig *config);
void vrrp_free(VrrpHandle *handle);
int vrrp_run(VrrpHandle *handle);
void *vrrp_run_async(VrrpHandle *handle);
CVrrpState vrrp_get_state(const VrrpHandle *handle);
bool vrrp_get_stats(const VrrpHandle *handle, CVrrpStats *stats);
int vrrp_shutdown(VrrpHandle *handle);
const char *vrrp_last_error(void);

#ifdef __cplusplus
}
#endif

#endif  /* VRRP_FFI_H */
