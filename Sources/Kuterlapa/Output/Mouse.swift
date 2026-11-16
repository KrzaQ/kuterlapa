import ApplicationServices
import Foundation

/// Moves the pointer, scrolls and clicks. Needs the Accessibility
/// permission, like Keystrokes.
enum Mouse {
    /// A tool such as Scroll Reverser flips the wheel events it takes for a
    /// mouse, ours too. The sticks' "scroll-inverted" role says so; then a
    /// model's scroll is flipped as well, so "down" shows what is below.
    static var wheelReversed = false

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

    /// Numbers click pairs: a real mouse gives the down and the up of one
    /// click the same event number.
    private static var clickNumber: Int64 = 0

    /// Clicks like a real mouse: the down and the up of one click share an
    /// event number, the down has pressure, and the button stays down for a
    /// moment. A remote desktop client (RustDesk) took the bare pairs, sent
    /// in the same millisecond, for double clicks.
    static func click(_ button: CGMouseButton = .left, count: Int = 1) {
        let (down, up): (CGEventType, CGEventType) =
            button == .right ? (.rightMouseDown, .rightMouseUp) : (.leftMouseDown, .leftMouseUp)
        let source = CGEventSource(stateID: .hidSystemState)
        let at = location
        for n in 1...count {
            clickNumber += 1
            for type in [down, up] {
                let event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: at,
                                    mouseButton: button)
                event?.setIntegerValueField(.mouseEventClickState, value: Int64(n))
                event?.setIntegerValueField(.mouseEventNumber, value: clickNumber)
                event?.setDoubleValueField(.mouseEventPressure, value: type == down ? 1.0 : 0.0)
                event?.post(tap: .cghidEventTap)
                usleep(type == down ? 40_000 : 0)
            }
            // A double click: the second comes soon, but not at once.
            if n < count { usleep(60_000) }
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
