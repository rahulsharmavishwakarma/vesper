#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Carbon/Carbon.h>
#import <AVFoundation/AVFoundation.h>
#include <sys/sysctl.h>
#include <math.h>
#import "projects.m"

// This is a separately owned NSPanel, not an isa replacement of TaoWindow.
// Its WebView remains associated with Tauri for IPC. Geometry/show/hide are
// exclusively owned here; the hidden original window keeps its delegate/view.
@interface TDPanel : NSPanel
@property (strong) NSEvent *dragMouseDown;
@property BOOL userPositioned;
@property BOOL dragging;
@end
@implementation TDPanel
- (BOOL)canBecomeKeyWindow { return NO; }
- (BOOL)canBecomeMainWindow { return NO; }
- (void)sendEvent:(NSEvent *)event {
    if (event.type == NSEventTypeLeftMouseDown) self.dragMouseDown = event;
    if (event.type == NSEventTypeLeftMouseUp) self.dragMouseDown = nil;
    [super sendEvent:event];
}
@end
static TDPanel *panel;
static NSWindow *originalWindow;
static NSView *originalContent;
static AXUIElementRef target;
static pid_t targetPID;
static CFRange targetSelection;
static uint64_t targetToken;
// Cells carry no text selection: the selected cell itself is the destination.
static bool targetIsCell;
static id pasteAccessibilityObserver;
// Opt-in read-back of one paste (docs/personalization/app-learning.md): the
// field and where the pasted text began. Only that span is ever read, once.
static AXUIElementRef receipt;
static pid_t receiptPID;
static CFIndex receiptLocation;
static CFIndex receiptLength;
static uint64_t receiptToken;
static bool receiptsEnabled;
void td_receipt_clear(void);
static NSMutableArray<NSRunningApplication *> *preparedPasteApplications;

// Electron exposes its editable tree asynchronously after this documented
// request. Prepare on activation, before the user starts dictating. Sending
// true again restarts Electron's debounce, so retain one success per live app.
// Do not read field contents, poll, or disable another client's accessibility.
static void preparePasteApplication(NSRunningApplication *application) {
    if (![NSThread isMainThread] || !pasteAccessibilityObserver || !application || application.terminated ||
        application.processIdentifier == getpid() || !AXIsProcessTrusted()) return;
    NSIndexSet *terminated = [preparedPasteApplications indexesOfObjectsPassingTest:
        ^BOOL(NSRunningApplication *candidate, NSUInteger index, BOOL *stop) {
            (void)index; (void)stop;
            return candidate.terminated;
        }];
    [preparedPasteApplications removeObjectsAtIndexes:terminated];
    if ([preparedPasteApplications containsObject:application] ||
        preparedPasteApplications.count >= 64) return;
    AXUIElementRef app = AXUIElementCreateApplication(application.processIdentifier);
    if (!app) return;
    AXUIElementSetMessagingTimeout(app, 0.15f);
    Boolean settable = false;
    if (AXUIElementIsAttributeSettable(app, CFSTR("AXManualAccessibility"), &settable)
            == kAXErrorSuccess && settable &&
        AXUIElementSetAttributeValue(app, CFSTR("AXManualAccessibility"), kCFBooleanTrue)
            == kAXErrorSuccess) {
        [preparedPasteApplications addObject:application];
    }
    CFRelease(app);
}

void td_paste_access_enabled(bool enabled) {
    if (![NSThread isMainThread]) return;
    NSNotificationCenter *center = NSWorkspace.sharedWorkspace.notificationCenter;
    if (!enabled) {
        if (pasteAccessibilityObserver) [center removeObserver:pasteAccessibilityObserver];
        pasteAccessibilityObserver = nil;
        // Keep successful requests across preference toggles: enabling again
        // must not restart an in-flight provider's readiness delay.
        return;
    }
    if (!preparedPasteApplications) preparedPasteApplications = [NSMutableArray array];
    if (!pasteAccessibilityObserver) {
        pasteAccessibilityObserver = [center addObserverForName:NSWorkspaceDidActivateApplicationNotification
            object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *notification) {
                preparePasteApplication(notification.userInfo[NSWorkspaceApplicationKey]);
            }];
    }
    preparePasteApplication(NSWorkspace.sharedWorkspace.frontmostApplication);
}

static NSArray<NSPasteboardItem *> *pendingClipboard;
static NSInteger pendingOwnership;
static bool restoreSnapshot(NSPasteboard *board, NSInteger ownership, NSArray<NSPasteboardItem *> *saved) {
    if (board.changeCount != ownership) return false;
    [board clearContents];
    return saved.count == 0 || [board writeObjects:saved];
}
static void restoreClipboard(void) {
    if (!pendingClipboard) return;
    restoreSnapshot(NSPasteboard.generalPasteboard, pendingOwnership, pendingClipboard);
    pendingClipboard = nil;
}

bool td_panel_init(void *view) {
    if (![NSThread isMainThread] || !view) return false;
    if (panel) return true;
    NSView *nativeView = (__bridge NSView *)view;
    originalWindow = nativeView.window;
    originalContent = originalWindow.contentView;
    if (!originalWindow || !originalContent) return false;
    NSRect frame = NSMakeRect(0, 0, 396, 192);
    panel = [[TDPanel alloc] initWithContentRect:frame
        styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
        backing:NSBackingStoreBuffered defer:NO];
    panel.releasedWhenClosed = NO;
    panel.floatingPanel = YES;
    panel.hidesOnDeactivate = NO;
    panel.becomesKeyOnlyIfNeeded = YES;
    panel.level = NSFloatingWindowLevel;
    panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
        NSWindowCollectionBehaviorFullScreenAuxiliary;
    panel.opaque = NO;
    panel.backgroundColor = NSColor.clearColor;
    panel.hasShadow = NO;
    panel.movable = YES;
    panel.movableByWindowBackground = NO;
    [originalWindow orderOut:nil];
    [originalWindow makeFirstResponder:nil];
    originalWindow.contentView = [[NSView alloc] initWithFrame:frame];
    originalContent.frame = frame;
    panel.contentView = originalContent;
    return true;
}
static NSRect frameWithinScreen(NSRect frame, NSRect available) {
    frame.origin.x = MAX(NSMinX(available), MIN(frame.origin.x, NSMaxX(available) - frame.size.width));
    frame.origin.y = MAX(NSMinY(available), MIN(frame.origin.y, NSMaxY(available) - frame.size.height));
    return frame;
}
static bool validDragEvent(NSEvent *event) {
    return event && event.type == NSEventTypeLeftMouseDown && event.windowNumber == panel.windowNumber
        && NSProcessInfo.processInfo.systemUptime - event.timestamp >= 0
        && NSProcessInfo.processInfo.systemUptime - event.timestamp < 1;
}
bool td_panel_is_dragging(void) {
    if (![NSThread isMainThread]) return false;
    // Window Server owns the gesture and may consume mouse-up entirely.
    if (!panel.visible || !(NSEvent.pressedMouseButtons & 1)) panel.dragging = NO;
    return panel.dragging;
}
bool td_panel_start_drag(void) {
    if (![NSThread isMainThread] || !panel.visible || td_panel_is_dragging()) return false;
    NSEvent *event = panel.dragMouseDown;
    panel.dragMouseDown = nil;
    // A delayed IPC after a quick click must not start a stale drag gesture.
    if (!validDragEvent(event) || !(NSEvent.pressedMouseButtons & 1)) return false;
    panel.dragging = YES;
    panel.userPositioned = YES;
    // Returns immediately, before the user finishes moving the panel.
    [panel performWindowDragWithEvent:event];
    return true;
}
void td_panel_resize(double width, double height) {
    if (![NSThread isMainThread] || !panel || !isfinite(width) || !isfinite(height) ||
        width < 120 || width > 600 || height < 64 || height > 400) return;
    NSRect frame = panel.frame;
    CGFloat centerX = NSMidX(frame);
    frame.size = NSMakeSize(width, height);
    frame.origin.x = centerX - width / 2;
    if (panel.visible && panel.screen) {
        NSRect available = panel.screen.visibleFrame;
        frame = frameWithinScreen(frame, available);
    }
    [panel setFrame:frame display:YES];
    // The borrowed WebView follows the panel, never the hidden Tauri window.
    originalContent.frame = NSMakeRect(0, 0, width, height);
}
void td_panel_show(void) {
    if (![NSThread isMainThread] || !panel) return;
    if (!panel.visible) {
        NSPoint pointer = NSEvent.mouseLocation;
        NSScreen *screen = NSScreen.mainScreen;
        for (NSScreen *candidate in NSScreen.screens) {
            if (NSPointInRect(pointer, candidate.frame)) { screen = candidate; break; }
        }
        if (panel.userPositioned) {
            // Keep the chosen position on its display; recover if that display
            // was removed or the menu bar / Dock changed the available bounds.
            CGFloat largest = 0;
            for (NSScreen *candidate in NSScreen.screens) {
                NSRect overlap = NSIntersectionRect(panel.frame, candidate.visibleFrame);
                CGFloat area = overlap.size.width * overlap.size.height;
                if (area > largest) { screen = candidate; largest = area; }
            }
            [panel setFrame:frameWithinScreen(panel.frame, screen.visibleFrame) display:YES];
        } else {
            NSRect available = screen.visibleFrame;
            [panel setFrameOrigin:NSMakePoint(NSMidX(available) - panel.frame.size.width / 2,
                                             NSMinY(available) + 36)];
        }
    }
    [panel orderFrontRegardless];
}
void td_panel_hide(void) {
    if ([NSThread isMainThread]) { panel.dragging = NO; [panel orderOut:nil]; }
}
void td_target_clear(uint64_t token) {
    if (![NSThread isMainThread] || (token && token != targetToken)) return;
    if (target) CFRelease(target);
    target = NULL; targetToken = 0; targetPID = 0; targetIsCell = false;
}
void td_panel_destroy(void) {
    if (![NSThread isMainThread]) return;
    td_paste_access_enabled(false);
    preparedPasteApplications = nil;
    td_target_clear(0);
    td_receipt_clear();
    restoreClipboard();
    [panel orderOut:nil];
    if (originalWindow && originalContent) {
        panel.contentView = [[NSView alloc] initWithFrame:panel.frame];
        originalWindow.contentView = originalContent;
    }
    [panel close]; panel = nil; originalContent = nil; originalWindow = nil;
}
uint32_t td_permissions(bool request) {
    if (![NSThread isMainThread]) return 1;
    NSDictionary *options = @{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @(request)};
    bool ax = AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
    bool post = CGPreflightPostEventAccess();
    if (request && ax && !post) post = CGRequestPostEventAccess();
    bool clipboard = true;
    if (@available(macOS 15.4, *)) {
        NSPasteboard *board = NSPasteboard.generalPasteboard;
        if (request && ax && post && board.accessBehavior != NSPasteboardAccessBehaviorAlwaysAllow) {
            // Explicit Allow access action only. Never trigger this prompt during
            // a recording or delivery, where it could steal the target's focus.
            (void)[board stringForType:NSPasteboardTypeString];
        }
        clipboard = board.accessBehavior == NSPasteboardAccessBehaviorAlwaysAllow;
    }
    return 1 | (ax ? 2 : 0) | (post ? 4 : 0) | (clipboard ? 8 : 0);
}
static id attribute(AXUIElementRef element, CFStringRef name) {
    CFTypeRef value = NULL;
    if (AXUIElementCopyAttributeValue(element, name, &value) != kAXErrorSuccess) return nil;
    return CFBridgingRelease(value);
}
static bool selectedRange(AXUIElementRef element, CFRange *range) {
    CFTypeRef value = NULL;
    if (AXUIElementCopyAttributeValue(element, kAXSelectedTextRangeAttribute, &value) != kAXErrorSuccess)
        return false;
    bool valid = CFGetTypeID(value) == AXValueGetTypeID() &&
        AXValueGetType((AXValueRef)value) == kAXValueCFRangeType &&
        AXValueGetValue((AXValueRef)value, kAXValueCFRangeType, range);
    CFRelease(value);
    return valid && range->location >= 0 && range->length >= 0;
}
static bool editable(AXUIElementRef element) {
    NSString *role = attribute(element, kAXRoleAttribute);
    NSString *subrole = attribute(element, kAXSubroleAttribute);
    if ([subrole isEqualToString:(__bridge NSString *)kAXSecureTextFieldSubrole]) return false;
    if (![@[(__bridge NSString *)kAXTextFieldRole, (__bridge NSString *)kAXTextAreaRole,
             (__bridge NSString *)kAXComboBoxRole] containsObject:role ?: @""]) return false;
    NSNumber *enabled = attribute(element, kAXEnabledAttribute);
    Boolean settable = false;
    // TextEdit's NSTextView can omit AXEnabled while advertising a writable
    // AXValue. Do not interpret a missing optional attribute as explicitly false.
    pid_t pid = 0;
    AXUIElementGetPid(element, &pid);
    bool textEditArea = !enabled && [role isEqualToString:(__bridge NSString *)kAXTextAreaRole] &&
        [[NSRunningApplication runningApplicationWithProcessIdentifier:pid].bundleIdentifier isEqualToString:@"com.apple.TextEdit"];
    return (enabled.boolValue || textEditArea) && AXUIElementIsAttributeSettable(element, kAXValueAttribute, &settable)
        == kAXErrorSuccess && settable;
}
static AXUIElementRef focused(pid_t pid) {
    AXUIElementRef app = AXUIElementCreateApplication(pid);
    AXUIElementSetMessagingTimeout(app, 0.15f);
    CFTypeRef result = NULL;
    AXError error = AXUIElementCopyAttributeValue(app, kAXFocusedUIElementAttribute, &result);
    CFRelease(app);
    if (error != kAXErrorSuccess || !result) return NULL;
    if (CFGetTypeID(result) != AXUIElementGetTypeID()) { CFRelease(result); return NULL; }
    AXUIElementSetMessagingTimeout((AXUIElementRef)result, 0.15f);
    return (AXUIElementRef)result;
}
// Bounds for the focused-subtree search below. Chromium apps materialize
// their accessibility tree lazily, so the first pass can see a wrapper with
// no children yet; one short retry lets it settle. The budgets keep a hung
// target app from stalling capture: every attribute read already carries a
// 0.15s messaging timeout, and the walk stops at the first usable field.
static const int kResolveMaxDepth = 6;
static const int kResolveMaxNodes = 120;
// Depth-first, tree-order walk for the field the user actually focused. The
// search never leaves the focused element's subtree, so a compose window with
// several fields (To, Subject, Body) can never resolve to one the user was
// not interacting with. Returns a retained reference or NULL.
static AXUIElementRef resolveFieldVisit(AXUIElementRef element, CFRange *selection, int depth, int *budget,
                                       int *childrenSeen) {
    if (!element || depth > kResolveMaxDepth || *budget <= 0) return NULL;
    (*budget)--;
    if (editable(element) && selectedRange(element, selection)) return (AXUIElementRef)CFRetain(element);
    if (depth == kResolveMaxDepth) return NULL;
    id children = attribute(element, kAXChildrenAttribute);
    if (![children isKindOfClass:[NSArray class]]) return NULL;
    (*childrenSeen)++;
    for (id child in (NSArray *)children) {
        if ((__bridge CFTypeRef)child == NULL || CFGetTypeID((__bridge CFTypeRef)child) != AXUIElementGetTypeID())
            continue;
        CFRange found = {0, 0};
        AXUIElementRef match =
            resolveFieldVisit((__bridge AXUIElementRef)child, &found, depth + 1, budget, childrenSeen);
        if (match) { *selection = found; return match; }
        if (*budget <= 0) return NULL;
    }
    return NULL;
}
static AXUIElementRef resolveField(AXUIElementRef root, CFRange *selection, bool patient) {
    int budget = kResolveMaxNodes, childrenSeen = 0;
    AXUIElementRef match = resolveFieldVisit(root, selection, 0, &budget, &childrenSeen);
    // Retry only when the first pass saw no children at all: a populated but
    // fieldless subtree (a toolbar, a Send button's container) is a genuine
    // miss, while a childless wrapper may be a not-yet-materialized tree.
    if (!match && patient && childrenSeen == 0) {
        [NSThread sleepForTimeInterval:0.12];
        budget = kResolveMaxNodes;
        childrenSeen = 0;
        match = resolveFieldVisit(root, selection, 0, &budget, &childrenSeen);
    }
    return match;
}
// Spreadsheet grids expose the selected cell — not a text field — as the
// focused element, and Command-V fills exactly that selection. Accept cells
// only in known spreadsheet hosts so a row selection in any other app can
// never become a paste destination.
static bool spreadsheetCell(AXUIElementRef element, NSString *bundleIdentifier) {
    if (![@[@"com.microsoft.Excel", @"com.apple.iWork.Numbers"] containsObject:bundleIdentifier ?: @""])
        return false;
    NSString *role = attribute(element, kAXRoleAttribute);
    return [role isEqualToString:(__bridge NSString *)kAXCellRole];
}
bool td_secure_input(void) {
    if (![NSThread isMainThread]) return false;
    return IsSecureEventInputEnabled();
}
// Why the last capture found no field: AX roles and flags only, never text.
static char captureFailure[192];
static void noteCaptureFailure(AXUIElementRef element, const char *reason) {
    if (!element) { snprintf(captureFailure, sizeof captureFailure, "%s", reason); return; }
    NSString *role = attribute(element, kAXRoleAttribute), *subrole = attribute(element, kAXSubroleAttribute);
    NSNumber *enabled = attribute(element, kAXEnabledAttribute);
    Boolean settable = false;
    AXError settableError = AXUIElementIsAttributeSettable(element, kAXValueAttribute, &settable);
    CFRange range;
    snprintf(captureFailure, sizeof captureFailure, "%s role=%s subrole=%s enabled=%s valueSettable=%s range=%s",
        reason, [role isKindOfClass:NSString.class] ? role.UTF8String : "-",
        [subrole isKindOfClass:NSString.class] ? subrole.UTF8String : "-",
        [enabled isKindOfClass:NSNumber.class] ? (enabled.boolValue ? "yes" : "no") : "missing",
        settableError ? "error" : (settable ? "yes" : "no"), selectedRange(element, &range) ? "yes" : "no");
}
int td_capture_failure(char *output, size_t size) {
    if (![NSThread isMainThread] || !output || !size || !captureFailure[0]) return -1;
    size_t length = strnlen(captureFailure, sizeof captureFailure);
    if (length >= size) return -1;
    memcpy(output, captureFailure, length + 1);
    return (int)length;
}
int td_target_capture(uint64_t token) {
    if (![NSThread isMainThread]) return -1;
    td_target_clear(0);
    captureFailure[0] = '\0';
    if ((td_permissions(false) & 6) != 6) return -1;
    // Secure input is a separate, global state (any password prompt, Touch ID,
    // or sudo session anywhere); attributing it to Accessibility sends users
    // to the wrong Settings pane, so it gets its own code.
    if (IsSecureEventInputEnabled()) return -4;
    NSRunningApplication *front = NSWorkspace.sharedWorkspace.frontmostApplication;
    // Terminals require independent bracketed-paste/command execution qualification.
    if (!front || front.processIdentifier == getpid() ||
        [@[@"com.apple.Terminal", @"com.googlecode.iterm2", @"dev.warp.Warp-Stable"]
            containsObject:front.bundleIdentifier ?: @""]) {
        noteCaptureFailure(NULL, front ? "terminal-excluded" : "no-app");
        return -2;
    }
    // Recover a request skipped before Accessibility permission was granted.
    // This never waits or adopts a field later in the recording: an unready
    // destination still uses Copy fallback for this operation.
    preparePasteApplication(front);
    AXUIElementRef element = focused(front.processIdentifier);
    if (!element) {
        noteCaptureFailure(NULL, "no-focused-element");
        return -2;
    }
    CFRange selection = {0, 0};
    AXUIElementRef field = resolveField(element, &selection, true);
    if (field) {
        target = field; targetPID = front.processIdentifier; targetSelection = selection; targetToken = token;
        targetIsCell = false;
        CFRelease(element);
        return 0;
    }
    if (spreadsheetCell(element, front.bundleIdentifier)) {
        target = element; targetPID = front.processIdentifier; targetSelection = selection; targetToken = token;
        targetIsCell = true;
        return 0;
    }
    noteCaptureFailure(element, "no-editable-field");
    CFRelease(element);
    return -2;
}
static bool targetStillMatches(uint64_t token) {
    if (!target || token != targetToken || IsSecureEventInputEnabled() ||
        NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier != targetPID) return false;
    AXUIElementRef current = focused(targetPID);
    if (!current) return false;
    bool same = false;
    if (targetIsCell) {
        NSString *role = attribute(current, kAXRoleAttribute);
        same = CFEqual(current, target) &&
            [role isEqualToString:(__bridge NSString *)kAXCellRole];
    } else {
        CFRange selection = {0, 0};
        AXUIElementRef resolved = resolveField(current, &selection, false);
        same = resolved && CFEqual(resolved, target) && editable(resolved) &&
            selectedRange(resolved, &selection) &&
            selection.location == targetSelection.location && selection.length == targetSelection.length;
        if (resolved) CFRelease(resolved);
    }
    CFRelease(current);
    return same;
}
static NSArray<NSPasteboardItem *> *snapshot(NSPasteboard *board) {
    NSMutableArray *saved = [NSMutableArray array];
    NSUInteger bytes = 0;
    NSArray *items = board.pasteboardItems;
    if (!items && board.types.count) return nil;
    for (NSPasteboardItem *item in items) {
        NSPasteboardItem *copy = [[NSPasteboardItem alloc] init];
        for (NSPasteboardType type in item.types) {
            NSData *data = [item dataForType:type];
            if (!data || data.length > 16 * 1024 * 1024 - bytes) return nil;
            bytes += data.length;
            if (![copy setData:data forType:type]) return nil;
        }
        [saved addObject:copy];
    }
    return saved;
}
// Resolve the Command-V key through the active layout; a hardcoded physical V
// can invoke another command on Dvorak or other non-QWERTY layouts.
static int pasteKeyCode(void) {
    TISInputSourceRef source = TISCopyCurrentKeyboardLayoutInputSource();
    if (!source) return -1;
    CFDataRef data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
    int found = -1;
    if (data && CFGetTypeID(data) == CFDataGetTypeID() && CFDataGetLength(data) > 0) {
        const UCKeyboardLayout *layout = (const UCKeyboardLayout *)CFDataGetBytePtr(data);
        for (UInt16 key = 0; key < 128; ++key) {
            UInt32 dead = 0; UniChar chars[4]; UniCharCount length = 0;
            OSStatus result = UCKeyTranslate(layout, key, kUCKeyActionDown, cmdKey >> 8,
                LMGetKbdType(), kUCKeyTranslateNoDeadKeysMask, &dead, 4, &length, chars);
            if (result == noErr && length == 1 && (chars[0] == 'v' || chars[0] == 'V')) {
                found = key; break;
            }
        }
    }
    CFRelease(source);
    return found;
}
int td_paste(uint64_t token, const char *text) {
    if (![NSThread isMainThread] || (td_permissions(false) & 6) != 6) return -1;
    if (!targetStillMatches(token)) return -2;
    CGEventFlags held = CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState);
    if (held & (kCGEventFlagMaskCommand | kCGEventFlagMaskControl | kCGEventFlagMaskAlternate |
                kCGEventFlagMaskShift | kCGEventFlagMaskSecondaryFn)) return 1;
    NSString *value = text ? [NSString stringWithUTF8String:text] : nil;
    if (!value.length) return -1;
    NSPasteboard *board = NSPasteboard.generalPasteboard;
    if (@available(macOS 15.4, *)) {
        if (board.accessBehavior != NSPasteboardAccessBehaviorAlwaysAllow) return -3;
    }
    if (pendingClipboard && board.changeCount == pendingOwnership) return 1;
    pendingClipboard = nil;
    NSInteger before = board.changeCount;
    NSArray *saved = snapshot(board);
    if (!saved || board.changeCount != before || !targetStillMatches(token)) return -3;
    int key = pasteKeyCode();
    if (key < 0) return -1;
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key, true);
    CGEventRef up = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key, false);
    if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return -1; }
    CGEventSetFlags(down, kCGEventFlagMaskCommand);
    // The synthetic key-up must release our Command flag; otherwise subsequent
    // dispatches can mistake it for a still-held physical modifier.
    CGEventSetFlags(up, 0);
    NSInteger cleared = [board clearContents];
    if (![board setString:value forType:NSPasteboardTypeString]) {
        restoreSnapshot(board, cleared, saved);
        CFRelease(down); CFRelease(up); return -3;
    }
    NSInteger owned = board.changeCount;
    pendingClipboard = saved; pendingOwnership = owned;
    if (receiptsEnabled && !targetIsCell) {
        if (receipt) CFRelease(receipt);
        receipt = (AXUIElementRef)CFRetain(target); receiptPID = targetPID;
        receiptLocation = targetSelection.location; receiptLength = (CFIndex)value.length;
        receiptToken = token;
    }
    // Consume before posting: another call can never duplicate this dispatch.
    td_target_clear(token);
    CGEventPost(kCGHIDEventTap, down); CGEventPost(kCGHIDEventTap, up);
    CFRelease(down); CFRelease(up);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        // changeCount indicates ownership, not paste consumption. This delay is
        // best effort; never overwrite text copied by the user or another app.
        if (pendingClipboard && pendingOwnership == owned) restoreClipboard();
    });
    return 0;
}
// Text of `element` in [location, location + length), clamped to the field.
// Uses the parameterized range read; falls back to a bounded AXValue slice.
static NSString *rangeText(AXUIElementRef element, CFIndex location, CFIndex length) {
    if (location < 0 || length <= 0) return nil;
    NSNumber *count = attribute(element, kAXNumberOfCharactersAttribute);
    if ([count isKindOfClass:NSNumber.class]) {
        CFIndex total = count.longValue;
        if (location > total) return nil;
        length = MIN(length, total - location);
        if (length <= 0) return @"";
        CFRange range = CFRangeMake(location, length);
        AXValueRef parameter = AXValueCreate(kAXValueCFRangeType, &range);
        CFTypeRef value = NULL;
        AXError error = AXUIElementCopyParameterizedAttributeValue(element,
            kAXStringForRangeParameterizedAttribute, parameter, &value);
        CFRelease(parameter);
        if (error == kAXErrorSuccess && value && CFGetTypeID(value) == CFStringGetTypeID())
            return CFBridgingRelease(value);
        if (value) CFRelease(value);
    }
    NSString *whole = attribute(element, kAXValueAttribute);
    if (![whole isKindOfClass:NSString.class] || whole.length > 200000 || location > (CFIndex)whole.length)
        return nil;
    return [whole substringWithRange:NSMakeRange(location, MIN(length, (CFIndex)whole.length - location))];
}
static bool secureElement(AXUIElementRef element) {
    NSString *subrole = attribute(element, kAXSubroleAttribute);
    return [subrole isEqualToString:(__bridge NSString *)kAXSecureTextFieldSubrole] || IsSecureEventInputEnabled();
}
static int copyOut(NSString *text, char *output, size_t size) {
    NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
    if (!data || data.length >= size) return -1;
    memcpy(output, data.bytes, data.length);
    output[data.length] = '\0';
    return (int)data.length;
}
// Up to `limit` UTF-16 units of the captured field before its caret (opt-in
// field words). Returns bytes written, or -1 when nothing may be read.
int td_target_context(uint64_t token, size_t limit, char *output, size_t size) {
    if (![NSThread isMainThread] || !output || !size || !target || token != targetToken || targetIsCell ||
        secureElement(target)) return -1;
    CFIndex start = MAX(0, targetSelection.location - (CFIndex)limit);
    NSString *text = rangeText(target, start, targetSelection.location - start);
    return text ? copyOut(text, output, size) : -1;
}
void td_paste_receipts_enabled(bool enabled) {
    if (![NSThread isMainThread]) return;
    receiptsEnabled = enabled;
    if (!enabled) td_receipt_clear();
}
void td_receipt_clear(void) {
    if (![NSThread isMainThread]) return;
    if (receipt) CFRelease(receipt);
    receipt = NULL; receiptPID = 0; receiptLocation = 0; receiptLength = 0; receiptToken = 0;
}
// One-shot read-back of the last paste: the text from where it began, up to
// twice its length plus a margin. `token` names the paste (0 takes any).
// Returns bytes written; -1 when there is no receipt or it cannot be read.
int td_receipt_read(uint64_t token, uint64_t *pasted, char *output, size_t size) {
    if (![NSThread isMainThread] || !receipt || !output || !size || (token && token != receiptToken)) return -1;
    NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:receiptPID];
    int written = -1;
    if (app && !app.terminated && !secureElement(receipt)) {
        AXUIElementSetMessagingTimeout(receipt, 0.15f);
        NSString *text = rangeText(receipt, receiptLocation, MIN(receiptLength * 2 + 80, (CFIndex)32768));
        if (text) written = copyOut(text, output, size);
    }
    if (pasted) *pasted = receiptToken;
    td_receipt_clear();
    return written;
}
bool td_copy(const char *text) {
    if (![NSThread isMainThread] || !text) return false;
    NSString *value = [NSString stringWithUTF8String:text];
    if (!value) return false;
    [NSPasteboard.generalPasteboard clearContents];
    return [NSPasteboard.generalPasteboard setString:value forType:NSPasteboardTypeString];
}

// First-run setup (issue #270). Microphone: 0 not asked, 1 restricted,
// 2 denied, 3 allowed, 4 just asked (the system prompt is showing). Asking is
// explicit only; a read never prompts.
int32_t td_microphone(bool request) {
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    switch (status) {
        case AVAuthorizationStatusAuthorized: return 3;
        case AVAuthorizationStatusDenied: return 2;
        case AVAuthorizationStatusRestricted: return 1;
        case AVAuthorizationStatusNotDetermined:
        default:
            if (!request) return 0;
            [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) { (void)granted; }];
            return 4;
    }
}
// The chip name (for example "Apple M3") and physical memory in bytes.
uint64_t td_device(char *chip, size_t capacity) {
    if (chip && capacity) {
        size_t size = capacity;
        if (sysctlbyname("machdep.cpu.brand_string", chip, &size, NULL, 0) != 0) chip[0] = 0;
        chip[capacity - 1] = 0;
    }
    return NSProcessInfo.processInfo.physicalMemory;
}
// Space available for new files on the volume holding `path`, counting
// purgeable space macOS frees on demand; -1 when unknown.
int64_t td_free_space(const char *path) {
    if (!path) return -1;
    NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    while (url && ![url checkResourceIsReachableAndReturnError:nil]) {
        NSURL *parent = url.URLByDeletingLastPathComponent;
        if ([parent isEqual:url]) break;
        url = parent;
    }
    NSNumber *free = nil;
    if (![url getResourceValue:&free forKey:NSURLVolumeAvailableCapacityForImportantUsageKey error:nil] || !free)
        return -1;
    return free.longLongValue;
}
