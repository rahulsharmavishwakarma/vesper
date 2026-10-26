// Background-work conditions and thread policy for the term-inventory worker.
// Thread-safe reads of process/power state; no UI, no permissions, no prompts.
#import <Foundation/Foundation.h>
#import <IOKit/ps/IOPowerSources.h>
#import <IOKit/ps/IOPSKeys.h>
#include <pthread.h>
#include <pthread/qos.h>
#include <sys/resource.h>
#include <time.h>

// 0 nominal, 1 fair, 2 serious, 3 critical (NSProcessInfoThermalState).
int32_t td_thermal_state(void) {
    @autoreleasepool {
        return (int32_t)NSProcessInfo.processInfo.thermalState;
    }
}

bool td_low_power_mode(void) {
    @autoreleasepool {
        NSProcessInfo *info = NSProcessInfo.processInfo;
        if (![info respondsToSelector:@selector(isLowPowerModeEnabled)]) return false;
        return info.isLowPowerModeEnabled;
    }
}

// 1 on AC power, 0 on battery or UPS, -1 unknown.
int32_t td_on_ac_power(void) {
    CFTypeRef info = IOPSCopyPowerSourcesInfo();
    if (!info) return -1;
    CFStringRef type = IOPSGetProvidingPowerSourceType(info);
    int32_t result = -1;
    if (type) result = CFStringCompare(type, CFSTR(kIOPMACPowerKey), 0) == kCFCompareEqualTo ? 1 : 0;
    CFRelease(info);
    return result;
}

// Lower the calling thread to background QoS with throttled disk I/O.
bool td_background_thread(void) {
    bool qos = pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND, 0) == 0;
    bool io = setiopolicy_np(IOPOL_TYPE_DISK, IOPOL_SCOPE_THREAD, IOPOL_THROTTLE) == 0;
    return qos && io;
}

// CPU time consumed by the calling thread, in nanoseconds.
uint64_t td_thread_cpu_ns(void) {
    return clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID);
}
