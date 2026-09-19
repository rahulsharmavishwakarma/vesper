// Included by macos.m: all references and adjacent text remain native and transient.
typedef void (*TDObservationCallback)(uint64_t, const char *, const char *);
static TDObservationCallback observationCallback;
static uint64_t observationToken;
static AXUIElementRef observedField, observedWindow, observedApp;
static AXObserverRef observationObserver;
static NSString *emittedText, *leftGuard, *rightGuard, *settlingText;
static CFIndex insertionOffset, originalCount, replacedCount;
static bool acknowledged;
static NSTimeInterval observationStarted, settlingStarted;
static NSTimer *observationTimer;
static id activationObserver;
static uint64_t observationReads, observationMaxRead;
static const char *observationReason = "idle";
static int observationRegistrationError, observationRegistrationIndex;

static void stopObservation(void) {
    [observationTimer invalidate]; observationTimer = nil;
    if (activationObserver) [NSWorkspace.sharedWorkspace.notificationCenter removeObserver:activationObserver];
    activationObserver = nil;
    if (observationObserver) CFRelease(observationObserver);
    observationObserver = NULL;
    if (observedField) CFRelease(observedField);
    if (observedWindow) CFRelease(observedWindow);
    if (observedApp) CFRelease(observedApp);
    observedField = NULL; observedWindow = NULL; observedApp = NULL;
    emittedText = nil; leftGuard = nil; rightGuard = nil; settlingText = nil;
    TDObservationCallback callback = observationCallback;
    uint64_t token = observationToken;
    observationCallback = NULL; observationToken = 0;
    if (callback) callback(token, NULL, NULL);
}
void td_observation_cancel(void) { if ([NSThread isMainThread]) stopObservation(); }
void td_observation_arm(uint64_t token, TDObservationCallback callback) {
    if (![NSThread isMainThread]) return;
    stopObservation(); observationToken = token; observationCallback = callback;
}
static NSArray *boundedChildren(AXUIElementRef element, CFIndex limit) {
    CFIndex count = 0;
    if (AXUIElementGetAttributeValueCount(element, kAXChildrenAttribute, &count) != kAXErrorSuccess || count > limit || count < 0) return nil;
    CFArrayRef result = NULL;
    if (AXUIElementCopyAttributeValues(element, kAXChildrenAttribute, 0, count, &result) != kAXErrorSuccess) return nil;
    return CFBridgingRelease(result);
}
static bool plainTextEdit(AXUIElementRef app) {
    // Conservative English UI adapter: Apple documents the Format conversion
    // command. Never infer plain text from a filename, bundle ID or text-area role.
    id menuBar = attribute(app, kAXMenuBarAttribute);
    if (!menuBar || CFGetTypeID((__bridge CFTypeRef)menuBar) != AXUIElementGetTypeID()) return false;
    for (id item in boundedChildren((__bridge AXUIElementRef)menuBar, 16)) {
        if (![attribute((__bridge AXUIElementRef)item, kAXTitleAttribute) isEqual:@"Format"]) continue;
        NSArray *menus = boundedChildren((__bridge AXUIElementRef)item, 1);
        if (menus.count != 1) return false;
        bool richCommand = false;
        for (id entry in boundedChildren((__bridge AXUIElementRef)menus[0], 32)) {
            NSString *title = attribute((__bridge AXUIElementRef)entry, kAXTitleAttribute);
            if ([title isEqual:@"Make Plain Text"]) return false;
            if ([title isEqual:@"Make Rich Text"]) richCommand = [attribute((__bridge AXUIElementRef)entry, kAXEnabledAttribute) boolValue];
        }
        return richCommand;
    }
    return false;
}
static bool directKeyboardLayout(void) {
    TISInputSourceRef source = TISCopyCurrentKeyboardInputSource();
    if (!source) return false;
    CFTypeRef type = TISGetInputSourceProperty(source, kTISPropertyInputSourceType);
    bool direct = type && CFEqual(type, kTISTypeKeyboardLayout);
    CFRelease(source);
    // AX text notifications do not expose reliable marked-text composition state.
    return direct;
}
static bool characterCount(AXUIElementRef field, CFIndex *count) {
    id value = attribute(field, kAXNumberOfCharactersAttribute);
    if (![value isKindOfClass:NSNumber.class]) return false;
    long long n = [value longLongValue];
    if (n < 0 || n > INT32_MAX) return false;
    *count = (CFIndex)n; return true;
}
static NSString *boundedText(AXUIElementRef field, CFIndex start, CFIndex count) {
    if (start < 0 || count < 0 || count > 8256 || start > INT32_MAX - count) return nil;
    if (!count) return @"";
    CFRange range = CFRangeMake(start, count);
    AXValueRef parameter = AXValueCreate(kAXValueCFRangeType, &range);
    CFTypeRef text = NULL;
    observationReads++; observationMaxRead = MAX(observationMaxRead, (uint64_t)count);
    AXError error = AXUIElementCopyParameterizedAttributeValue(field, kAXStringForRangeParameterizedAttribute, parameter, &text);
    CFRelease(parameter);
    if (error != kAXErrorSuccess || !text) { if (text) CFRelease(text); return nil; }
    bool valid = CFGetTypeID(text) == CFStringGetTypeID() && CFStringGetLength(text) == count;
    if (!valid) { CFRelease(text); return nil; }
    return CFBridgingRelease(text);
}
// UTF-16 throughout. This pure function is also exercised through the Rust tests.
static NSString *interior(NSString *window, NSString *left, NSString *right) {
    if (!window || window.length > 8256 || left.length > 32 || right.length > 32 ||
        window.length < left.length + right.length || (left.length && ![window hasPrefix:left]) || (right.length && ![window hasSuffix:right])) return nil;
    for (NSString *guard in @[left, right]) {
        if (!guard.length) continue;
        NSRange first = [window rangeOfString:guard options:NSLiteralSearch];
        if (first.location == NSNotFound) return nil;
        // Include overlapping matches; repeated boundary text is ambiguous.
        if ([window rangeOfString:guard options:NSLiteralSearch range:NSMakeRange(first.location + 1, window.length - first.location - 1)].location != NSNotFound) return nil;
    }
    NSString *value = [window substringWithRange:NSMakeRange(left.length, window.length - left.length - right.length)];
    if (value.length > 8192 || ![value canBeConvertedToEncoding:NSUTF8StringEncoding] ||
        strlen(value.UTF8String) != [value lengthOfBytesUsingEncoding:NSUTF8StringEncoding]) return nil;
    return value;
}
bool td_observation_validate(const char *window, const char *left, const char *right, const char *expected) {
    if (!window || !left || !right || !expected) return false;
    NSString *value = interior([NSString stringWithUTF8String:window], [NSString stringWithUTF8String:left], [NSString stringWithUTF8String:right]);
    return value && [value isEqual:[NSString stringWithUTF8String:expected]];
}
void td_observation_metrics(uint64_t *reads, uint64_t *maximum) {
    if (![NSThread isMainThread]) return;
    if (reads) *reads = observationReads;
    if (maximum) *maximum = observationMaxRead;
}
static bool observationFocus(void) {
    if (!observedField || !AXIsProcessTrusted() || IsSecureEventInputEnabled() || !directKeyboardLayout()) return false;
    pid_t pid = 0; AXUIElementGetPid(observedField, &pid);
    if (NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier != pid) return false;
    AXUIElementRef current = focused(pid);
    bool same = current && CFEqual(current, observedField) && editable(current);
    if (current) CFRelease(current);
    id window = attribute(observedField, kAXWindowAttribute);
    return same && window && CFEqual((__bridge CFTypeRef)window, observedWindow) && plainTextEdit(observedApp);
}
// Pure transition gate. Native tests inject observations, never AX permissions.
static int observationStep(bool ack, NSString *before, NSString *value, CFIndex offset,
    CFIndex length, CFRange selection, double elapsed, bool focus, bool stable) {
    if (!focus || elapsed < 0 || elapsed > (ack ? 60 : 2) || offset < 0 || length < 0 || length > 8192 ||
        offset > INT32_MAX - length || selection.location < 0 || selection.length < 0) return -1;
    if (!ack) return value && [value isEqual:before] && selection.location == offset + length && selection.length == 0 ? 1 : 0;
    if (!value || selection.location < offset || selection.location > offset + length ||
        selection.length > offset + length - selection.location) return -1;
    if ([value isEqual:before]) return 0;
    if (!value.length || selection.length != 0) return -1;
    return stable ? 3 : 2;
}
int td_observation_decide(bool ack, const char *before, const char *window, const char *left, const char *right,
    int64_t offset, int64_t selectionStart, int64_t selectionLength, double elapsed, bool focus, bool stable) {
    if (!before || !window || !left || !right) return -1;
    NSString *value = interior([NSString stringWithUTF8String:window], [NSString stringWithUTF8String:left], [NSString stringWithUTF8String:right]);
    return observationStep(ack, [NSString stringWithUTF8String:before], value, offset, value.length,
        CFRangeMake(selectionStart, selectionLength), elapsed, focus, stable);
}
static void sampleObservation(void) {
    if (!observationFocus()) { observationReason="focus_or_eligibility"; stopObservation(); return; }
    NSTimeInterval now = NSProcessInfo.processInfo.systemUptime;
    if (now - observationStarted > (acknowledged ? 60 : 2)) { stopObservation(); return; }
    CFIndex total; CFRange selection;
    if (!characterCount(observedField, &total) || !selectedRange(observedField, &selection)) { stopObservation(); return; }
    CFIndex length = total - (originalCount - replacedCount);
    if (length < 0 || length > 8192 || insertionOffset + length > total) { stopObservation(); return; }
    NSString *window = boundedText(observedField, insertionOffset - leftGuard.length, leftGuard.length + length + rightGuard.length);
    NSString *value = interior(window, leftGuard, rightGuard);
    bool stable = [value isEqual:settlingText] && now - settlingStarted >= 0.29;
    int step = observationStep(acknowledged, emittedText, value, insertionOffset, length, selection,
        now - observationStarted, true, stable);
    if (step < 0) { stopObservation(); return; }
    if (step == 0) { settlingText = nil; return; }
    if (step == 1) {
        acknowledged = true; [observationTimer invalidate]; observationTimer = nil;
        uint64_t token = observationToken;
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 60 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
            if (observationToken == token) stopObservation();
        });
        return;
    }
    if (step == 2) {
        if (![value isEqual:settlingText]) { settlingText = value; settlingStarted = now; }
        uint64_t token = observationToken;
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 300 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
            if (token == observationToken) sampleObservation();
        });
        return;
    }
    TDObservationCallback callback = observationCallback;
    uint64_t token = observationToken;
    NSString *before = emittedText, *after = value;
    observationCallback = NULL;
    stopObservation();
    if (callback) callback(token, before.UTF8String, after.UTF8String);
}
static bool observationReadQueued;
static void observationEvent(AXObserverRef observer, AXUIElementRef element, CFStringRef notification, void *context) {
    (void)observer; (void)element; (void)context;
    if (CFEqual(notification, kAXUIElementDestroyedNotification) || CFEqual(notification, kAXFocusedWindowChangedNotification) ||
        CFEqual(notification, kAXFocusedUIElementChangedNotification)) { observationReason="identity_notification"; stopObservation(); return; }
    if (observationReadQueued) return;
    observationReadQueued = true;
    uint64_t token = observationToken;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 50 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
        observationReadQueued = false;
        if (token == observationToken) sampleObservation();
    });
}
static void beginObservation(uint64_t token, NSString *value) {
    if (observationToken != token || !observationCallback) return;
    observationReason="eligibility";
    NSRunningApplication *front = NSWorkspace.sharedWorkspace.frontmostApplication;
    if (![front.bundleIdentifier isEqual:@"com.apple.TextEdit"] || !value.length || value.length > 8192 || !directKeyboardLayout() ||
        ![attribute(target, kAXRoleAttribute) isEqual:(__bridge NSString *)kAXTextAreaRole]) { stopObservation(); return; }
    observedApp = AXUIElementCreateApplication(targetPID);
    AXUIElementSetMessagingTimeout(observedApp, 0.15f);
    if (!plainTextEdit(observedApp) || !characterCount(target, &originalCount) || targetSelection.location > originalCount ||
        targetSelection.length > originalCount - targetSelection.location) { stopObservation(); return; }
    id window = attribute(target, kAXWindowAttribute);
    if (!window || CFGetTypeID((__bridge CFTypeRef)window) != AXUIElementGetTypeID()) { stopObservation(); return; }
    observedWindow = (AXUIElementRef)CFRetain((__bridge CFTypeRef)window);
    observedField = (AXUIElementRef)CFRetain(target);
    insertionOffset = targetSelection.location; replacedCount = targetSelection.length;
    leftGuard = boundedText(target, MAX(0, insertionOffset - 32), MIN(32, insertionOffset));
    CFIndex right = insertionOffset + replacedCount;
    rightGuard = boundedText(target, right, MIN(32, originalCount - right));
    emittedText = value; acknowledged = false; settlingText = nil;
    if (!leftGuard || !rightGuard || !interior([NSString stringWithFormat:@"%@%@%@",leftGuard,value,rightGuard], leftGuard, rightGuard)) { stopObservation(); return; }
    observationReason="observer_create";
    if (AXObserverCreate(targetPID, observationEvent, &observationObserver) != kAXErrorSuccess) { stopObservation(); return; }
    struct { AXUIElementRef element; CFStringRef notification; } registrations[] = {
        { observedField, kAXValueChangedNotification }, { observedField, kAXSelectedTextChangedNotification },
        { observedField, kAXUIElementDestroyedNotification }, { observedWindow, kAXUIElementDestroyedNotification },
        { observedApp, kAXFocusedUIElementChangedNotification }, { observedApp, kAXFocusedWindowChangedNotification }
    };
    for (unsigned i=0; i<sizeof(registrations)/sizeof(registrations[0]); i++) {
        AXError error = AXObserverAddNotification(observationObserver, registrations[i].element, registrations[i].notification, NULL);
        if (error != kAXErrorSuccess && error != kAXErrorNotificationAlreadyRegistered) { observationReason="registration"; observationRegistrationError=error; observationRegistrationIndex=i; stopObservation(); return; }
    }
    CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observationObserver), kCFRunLoopCommonModes);
    activationObserver = [NSWorkspace.sharedWorkspace.notificationCenter addObserverForName:NSWorkspaceDidActivateApplicationNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
        (void)note; stopObservation();
    }];
    observationReason="waiting_ack";
    observationStarted = NSProcessInfo.processInfo.systemUptime;
    observationTimer = [NSTimer timerWithTimeInterval:0.05 repeats:YES block:^(NSTimer *timer) { (void)timer; sampleObservation(); }];
    [NSRunLoop.mainRunLoop addTimer:observationTimer forMode:NSRunLoopCommonModes];
}
