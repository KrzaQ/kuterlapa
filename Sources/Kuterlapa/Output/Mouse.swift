import ApplicationServices
import Foundation

/// Moves the pointer, scrolls and clicks. Needs the Accessibility
/// permission, like Keystrokes.
enum Mouse {
    static var location: CGPoint {
        CGEvent(source: nil)?.location ?? .zero
    }

    static func move(by delta: CGPoint) {
        var target = CGPoint(x: location.x + delta.x, y: location.y + delta.y)
        // Keep the pointer on a screen: the union of all displays.
        let bounds = allDisplays()
        target.x = min(max(target.x, bounds.minX), bounds.maxX - 1)
        target.y = min(max(target.y, bounds.minY), bounds.maxY - 1)
        CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: target,
                mouseButton: .left)?.post(tap: .cghidEventTap)
    }

    /// Scrolls by pixels; positive dy scrolls up.
    static func scroll(dx: Int32, dy: Int32) {
        CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                wheel1: dy, wheel2: dx, wheel3: 0)?.post(tap: .cghidEventTap)
    }

    static func click(_ button: CGMouseButton = .left, count: Int = 1) {
        let (down, up): (CGEventType, CGEventType) =
            button == .right ? (.rightMouseDown, .rightMouseUp) : (.leftMouseDown, .leftMouseUp)
        let at = location
        for n in 1...count {
            for type in [down, up] {
                let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: at,
                                    mouseButton: button)
                event?.setIntegerValueField(.mouseEventClickState, value: Int64(n))
                event?.post(tap: .cghidEventTap)
            }
        }
    }

    private static func allDisplays() -> CGRect {
        var count: UInt32 = 0
        CGGetActiveDisplayList(0, nil, &count)
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        CGGetActiveDisplayList(count, &ids, &count)
        return ids.reduce(CGRect.null) { $0.union(CGDisplayBounds($1)) }
    }
}
