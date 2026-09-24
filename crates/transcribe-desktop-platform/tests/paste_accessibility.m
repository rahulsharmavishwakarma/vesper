// Exercise native listener lifecycle without enabling accessibility in real
// applications, permission prompts, clipboard access or synthetic key events.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <assert.h>

static bool trusted = true;
static bool supported = true;
static AXError requestResult = kAXErrorSuccess;
static unsigned requests;
static bool testTrusted(void) { return trusted; }
static AXError testSettable(AXUIElementRef app, CFStringRef name, Boolean *value) {
    assert(CFEqual(name, CFSTR("AXManualAccessibility")));
    *value = supported;
    return kAXErrorSuccess;
}
static AXError testSetValue(AXUIElementRef app, CFStringRef name, CFTypeRef value) {
    assert(CFEqual(name, CFSTR("AXManualAccessibility")));
    assert(value == kCFBooleanTrue); // Never disable somebody else's provider.
    requests++;
    return requestResult;
}
#define AXIsProcessTrusted testTrusted
#define AXUIElementIsAttributeSettable testSettable
#define AXUIElementSetAttributeValue testSetValue
#import "../src/macos.m"
#undef AXIsProcessTrusted
#undef AXUIElementIsAttributeSettable
#undef AXUIElementSetAttributeValue

@interface FixtureApplication : NSObject
@property pid_t processIdentifier;
@property(getter=isTerminated) BOOL terminated;
@end
@implementation FixtureApplication
@end
static NSRunningApplication *fixture(pid_t pid) {
    FixtureApplication *app = [FixtureApplication new];
    app.processIdentifier = pid;
    return (NSRunningApplication *)app;
}
static void activate(NSRunningApplication *app) {
    [NSWorkspace.sharedWorkspace.notificationCenter
        postNotificationName:NSWorkspaceDidActivateApplicationNotification object:nil
        userInfo:@{ NSWorkspaceApplicationKey: app }];
}

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSRunningApplication *app = fixture(20000001);
        activate(app);
        preparePasteApplication(app);
        assert(requests == 0 && !pasteAccessibilityObserver);

        // No trust: registering the listener must not request OS permission or
        // memoize the app as ready. A later activation can recover after grant.
        trusted = false;
        td_paste_access_enabled(true);
        id observer = pasteAccessibilityObserver;
        assert(observer);
        activate(app);
        assert(requests == 0 && preparedPasteApplications.count == 0);
        trusted = true;
        activate(app);
        assert(requests == 1 && [preparedPasteApplications containsObject:app]);
        for (int i = 0; i < 8; ++i) activate(app);
        assert(requests == 1); // Re-activation cannot restart the debounce.

        supported = false;
        td_paste_access_enabled(true);
        assert(pasteAccessibilityObserver == observer); // Idempotent registration.
        NSRunningApplication *second = fixture(20000002);
        activate(second);
        assert(requests == 1);
        supported = true;
        requestResult = kAXErrorCannotComplete;
        activate(second);
        assert(requests == 2 && ![preparedPasteApplications containsObject:second]);
        requestResult = kAXErrorSuccess;
        activate(second);
        assert(requests == 3 && [preparedPasteApplications containsObject:second]);

        // Teardown removes delivery; preference toggles retain successful
        // requests so fast disable/re-enable doesn't restart a pending timer.
        td_paste_access_enabled(false);
        activate(fixture(20000003));
        preparePasteApplication(fixture(20000003));
        assert(!pasteAccessibilityObserver && requests == 3);
        supported = false; // Ignore the real foreground app on re-enable.
        td_paste_access_enabled(true);
        supported = true;
        activate(app);
        assert(requests == 3);

        activate(fixture(getpid()));
        NSRunningApplication *dead = fixture(20000004);
        ((FixtureApplication *)dead).terminated = YES;
        activate(dead);
        assert(requests == 3);
        ((FixtureApplication *)app).terminated = YES;
        NSRunningApplication *replacement = fixture(app.processIdentifier);
        activate(replacement);
        assert(requests == 4 && ![preparedPasteApplications containsObject:app]);
        assert([preparedPasteApplications containsObject:replacement]);

        for (int i = 0; i < 80; ++i) activate(fixture(21000000 + i));
        assert(preparedPasteApplications.count == 64);
        unsigned bounded = requests;
        activate(fixture(22000000));
        assert(requests == bounded);
        ((FixtureApplication *)second).terminated = YES;
        activate(fixture(22000000));
        assert(preparedPasteApplications.count == 64 && requests == bounded + 1);

        // Every entry is main-thread-only, including a stale background caller.
        dispatch_semaphore_t finished = dispatch_semaphore_create(0);
        dispatch_async(dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), ^{
            td_paste_access_enabled(false);
            dispatch_semaphore_signal(finished);
        });
        assert(dispatch_semaphore_wait(finished, dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC)) == 0);
        assert(pasteAccessibilityObserver);
        td_panel_destroy();
        assert(!pasteAccessibilityObserver && !preparedPasteApplications);
        bounded = requests;
        activate(fixture(23000000));
        assert(requests == bounded);
        puts("PASS: opt-in paste accessibility, grant recovery, one request per live app, unsupported/transient failure, preference toggles, process replacement, bounded cache, main-thread guard and shutdown.");
    }
    return 0;
}
