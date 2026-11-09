import Foundation

/// Turns analog stick positions into pointer movement and scrolling, 60
/// times a second. The core says which stick does what (the "sticks"
/// request); the movement itself stays here, because a round trip to the
/// core for every frame would lag.
///
/// Roles: "mouse" moves the pointer, "scroll" scrolls (stick up shows what
/// is above), "scroll-inverted" scrolls the other way, anything else does
/// nothing. Tools such as Scroll Reverser flip wheel events they take for a
/// mouse, and the stick's events look like a mouse to them; then
/// "scroll-inverted" gives the expected direction. Speed grows with the square of the tilt, so a small tilt gives
/// fine control and a full tilt moves fast.
final class Sticks {
    private let gamepad: Gamepad
    private var timer: Timer?
    private var left = "", right = ""
    private var scrollRest = CGPoint.zero  // scroll fractions carried to the next frame

    private let deadZone: CGFloat = 0.15
    private let pointerSpeed: CGFloat = 1400  // points per second at full tilt
    private let scrollSpeed: CGFloat = 1200  // pixels per second at full tilt

    init(gamepad: Gamepad) {
        self.gamepad = gamepad
    }

    /// Roles from the core: {"leftstick": "scroll", "rightstick": "mouse"}.
    func configure(_ roles: [String: Any]) {
        left = roles["leftstick"] as? String ?? ""
        right = roles["rightstick"] as? String ?? ""
        log("sticks: left=\(left.isEmpty ? "-" : left) right=\(right.isEmpty ? "-" : right)")
        if timer == nil, !(left.isEmpty && right.isEmpty) {
            timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
                self?.tick()
            }
        }
    }

    private func tick() {
        guard Keystrokes.allowed else { return }
        apply(left, gamepad.leftStick)
        apply(right, gamepad.rightStick)
    }

    private func apply(_ role: String, _ stick: CGPoint) {
        let v = CGPoint(x: shape(stick.x), y: shape(stick.y))
        guard v != .zero else { return }
        let frame: CGFloat = 1.0 / 60
        switch role {
        case "mouse":
            // Stick up is +y; screen coordinates grow downwards.
            Mouse.move(by: CGPoint(x: v.x * pointerSpeed * frame, y: -v.y * pointerSpeed * frame))
        case "scroll", "scroll-inverted":
            // While the chat view is open, the scroll stick scrolls it,
            // wherever the pointer is. Stick up shows what is above.
            if let view = ShellCommands.chatView, view.isVisible {
                let dy = -v.y * scrollSpeed * frame
                if abs(dy) >= 1 { view.scroll(String(Double(dy))) }
                return
            }
            let sign: CGFloat = role == "scroll" ? 1 : -1
            scrollRest.x += sign * v.x * scrollSpeed * frame
            scrollRest.y += sign * v.y * scrollSpeed * frame
            let dx = Int32(scrollRest.x), dy = Int32(scrollRest.y)
            scrollRest.x -= CGFloat(dx)
            scrollRest.y -= CGFloat(dy)
            // Stick up scrolls up: content moves down and shows what is
            // above. -dx: stick right shows content further right.
            if dx != 0 || dy != 0 { Mouse.scroll(dx: -dx, dy: dy) }
        default:
            break
        }
    }

    /// Dead zone, then a square curve, keeping the sign.
    private func shape(_ value: CGFloat) -> CGFloat {
        let magnitude = abs(value)
        guard magnitude > deadZone else { return 0 }
        let t = (magnitude - deadZone) / (1 - deadZone)
        return (value < 0 ? -1 : 1) * t * t
    }
}
