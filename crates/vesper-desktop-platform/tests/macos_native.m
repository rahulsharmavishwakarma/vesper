// Explicit native smoke: isolated pasteboard, ephemeral panel, no microphone,
// no permission prompts, no synthetic keystrokes into another application.
#import "../src/macos.m"
#import <WebKit/WebKit.h>
#include <assert.h>

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        NSPasteboard *board = [NSPasteboard pasteboardWithUniqueName];
        NSPasteboardItem *item = [[NSPasteboardItem alloc] init];
        NSData *binary = [NSData dataWithBytes:"\x00\x01\xff" length:3];
        assert([item setString:@"Original हिन्दी\nsecond line" forType:NSPasteboardTypeString]);
        assert([item setData:binary forType:@"org.transcribe.test.binary"]);
        assert([board writeObjects:@[item]]);
        NSArray *saved = snapshot(board);
        assert(saved.count == 1 && [saved[0] types].count == 2);
        [board clearContents]; [board setString:@"temporary" forType:NSPasteboardTypeString];
        NSInteger ownership = board.changeCount;
        assert(restoreSnapshot(board, ownership, saved));
        assert([[board stringForType:NSPasteboardTypeString] isEqualToString:@"Original हिन्दी\nsecond line"]);
        assert([[board dataForType:@"org.transcribe.test.binary"] isEqualToData:binary]);
        [board clearContents]; [board setString:@"temporary again" forType:NSPasteboardTypeString];
        ownership = board.changeCount;
        [board clearContents]; [board setString:@"newer user copy" forType:NSPasteboardTypeString];
        assert(!restoreSnapshot(board, ownership, saved));
        assert([[board stringForType:NSPasteboardTypeString] isEqualToString:@"newer user copy"]);
        [board clearContents];
        assert(snapshot(board).count == 0);
        [board releaseGlobally];

        assert(pasteKeyCode() >= 0);
        NSInteger untouched = NSPasteboard.generalPasteboard.changeCount;
        int noTarget = td_paste(999, "Never insert without a target");
        assert(noTarget == -1 || noTarget == -2);
        assert(NSPasteboard.generalPasteboard.changeCount == untouched);
        pid_t front = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
        NSWindow *host = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 440, 96)
            styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        host.releasedWhenClosed = NO;
        NSView *content = host.contentView;
        WKWebView *web = [[WKWebView alloc] initWithFrame:content.bounds];
        web.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [content addSubview:web];
        [web loadHTMLString:@"<body style='background:#171d18;color:#ecede7;font:16px system-ui'>Native recorder validation</body>" baseURL:nil];
        assert(td_panel_init((__bridge void *)content));
        assert(panel.contentView == content && host.contentView != nil && host.contentView != content);
        assert([panel isKindOfClass:NSPanel.class]);
        assert(!panel.canBecomeKeyWindow && !panel.canBecomeMainWindow);
        assert(panel.styleMask & NSWindowStyleMaskNonactivatingPanel);
        td_panel_show();
        assert(panel.visible && !host.visible);
        assert(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier == front);
        const NSSize sizes[] = { {396,192}, {388,148}, {454,224}, {412,232}, {272,280}, {200,304} };
        for (int i = 0; i < 6; ++i) {
            td_panel_resize(sizes[i].width, sizes[i].height);
            assert(NSEqualSizes(panel.frame.size, sizes[i]));
            assert(NSEqualSizes(content.frame.size, sizes[i]));
            assert(NSEqualSizes(web.frame.size, sizes[i]));
            assert(!panel.canBecomeKeyWindow && !panel.canBecomeMainWindow);
            assert(NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier == front);
        }
        td_panel_resize(NAN, 200);
        td_panel_resize(0, 0);
        assert(NSEqualSizes(panel.frame.size, sizes[5]));
        assert(!td_panel_start_drag()); // No mouse-down: IPC cannot invent a drag.
        assert(!td_panel_is_dragging());
        NSEvent *oldDown = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown
            location:NSMakePoint(20, 20) modifierFlags:0 timestamp:0
            windowNumber:panel.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
        assert(!validDragEvent(oldDown));
        NSRect safe = panel.screen.visibleFrame;
        NSRect chosen = frameWithinScreen(NSMakeRect(NSMinX(safe) + 60, NSMinY(safe) + 100,
            panel.frame.size.width, panel.frame.size.height), safe);
        panel.userPositioned = YES;
        [panel setFrame:chosen display:YES];
        td_panel_hide(); td_panel_show();
        assert(NSEqualRects(panel.frame, chosen));
        NSRect recovered = frameWithinScreen(NSMakeRect(-50000, 50000, 200, 304), safe);
        assert(NSContainsRect(safe, recovered));
        [panel setFrameOrigin:NSMakePoint(-50000, 50000)];
        td_panel_hide(); td_panel_show();
        assert(NSContainsRect(panel.screen.visibleFrame, panel.frame));
        // Desktop coordinates may be negative on a display left of the primary.
        NSRect leftScreen = NSMakeRect(-1920, -300, 1920, 1080);
        NSRect onLeft = NSMakeRect(-1500, 20, 396, 192);
        assert(NSEqualRects(frameWithinScreen(onLeft, leftScreen), onLeft));
        for (int i = 0; i < 20; ++i) { td_panel_hide(); td_panel_show(); }
        __block bool evaluated = false;
        __block bool valid = false;
        [web evaluateJavaScript:@"6 * 7" completionHandler:^(id result, NSError *error) {
            valid = !error && [result intValue] == 42; evaluated = true;
        }];
        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:8];
        while (!evaluated && deadline.timeIntervalSinceNow > 0)
            [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
        assert(evaluated && valid);
        td_panel_destroy();
        assert(host.contentView == content && web.superview == content);
        [host close];
        printf("PASS: native NSPanel hosting, all six sizes, drag-event guards, retained positions, offscreen recovery, 20 show/hide cycles, unchanged frontmost app, WebKit evaluation, restoration on teardown, multi-format clipboard, newer-copy preservation, empty clipboard.\n");
        printf("Native permission bits (no prompt): %u\n", td_permissions(false));
    }
    return 0;
}
