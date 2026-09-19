#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Carbon/Carbon.h>
#include <math.h>

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
void td_observation_cancel(void);
static TDPanel *panel;
static NSWindow *originalWindow;
static NSView *originalContent;
static AXUIElementRef target;
static pid_t targetPID;
static CFRange targetSelection;
static uint64_t targetToken;
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
    target = NULL; targetToken = 0; targetPID = 0;
}
void td_panel_destroy(void) {
    if (![NSThread isMainThread]) return;
    td_target_clear(0);
    td_observation_cancel();
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
int td_target_capture(uint64_t token) {
    if (![NSThread isMainThread]) return -1;
    td_target_clear(0);
    if ((td_permissions(false) & 6) != 6 || IsSecureEventInputEnabled()) return -1;
    NSRunningApplication *front = NSWorkspace.sharedWorkspace.frontmostApplication;
    // Terminals require independent bracketed-paste/command execution qualification.
    if (!front || front.processIdentifier == getpid() ||
        [@[@"com.apple.Terminal", @"com.googlecode.iterm2", @"dev.warp.Warp-Stable"]
            containsObject:front.bundleIdentifier ?: @""]) return -2;
    AXUIElementRef element = focused(front.processIdentifier);
    CFRange selection;
    if (!element) return -2;
    if (!editable(element) || !selectedRange(element, &selection)) { CFRelease(element); return -2; }
    target = element; targetPID = front.processIdentifier; targetSelection = selection; targetToken = token;
    return 0;
}
static bool targetStillMatches(uint64_t token) {
    if (!target || token != targetToken || IsSecureEventInputEnabled() ||
        NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier != targetPID) return false;
    AXUIElementRef current = focused(targetPID);
    if (!current) return false;
    CFRange selection;
    bool same = CFEqual(current, target) && editable(current) && selectedRange(current, &selection) &&
        selection.location == targetSelection.location && selection.length == targetSelection.length;
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
#include "observation.m"

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
    beginObservation(token, value);
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
bool td_copy(const char *text) {
    if (![NSThread isMainThread] || !text) return false;
    NSString *value = [NSString stringWithUTF8String:text];
    if (!value) return false;
    [NSPasteboard.generalPasteboard clearContents];
    return [NSPasteboard.generalPasteboard setString:value forType:NSPasteboardTypeString];
}
