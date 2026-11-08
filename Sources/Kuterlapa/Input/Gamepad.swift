import Foundation
import GameController

/// Reads game controller buttons and sticks, also while another app is in
/// front.
///
/// Button names are the labels on the controller, with " Button" removed:
/// A B X Y L R ZL ZR + - Share, Up Down Left Right for the d-pad, and L3 R3
/// for pressing the sticks. macOS keeps the Home button for itself, so it
/// never arrives here.
///
/// Each button reports both its press and its release; the core turns them
/// into single presses, double presses and holds.
final class Gamepad {
    private let onButton: (String, Bool) -> Void
    private var seen = Set<ObjectIdentifier>()
    private var down = Set<String>()

    /// The current position of the sticks, from -1 to 1 on each axis.
    private(set) var leftStick = CGPoint.zero
    private(set) var rightStick = CGPoint.zero

    init(onButton: @escaping (_ name: String, _ pressed: Bool) -> Void) {
        self.onButton = onButton
    }

    func start() {
        GCController.shouldMonitorBackgroundEvents = true
        let center = NotificationCenter.default
        center.addObserver(forName: .GCControllerDidConnect, object: nil, queue: .main) { [weak self] note in
            if let controller = note.object as? GCController { self?.attach(controller) }
        }
        center.addObserver(forName: .GCControllerDidDisconnect, object: nil, queue: .main) { [weak self] note in
            guard let controller = note.object as? GCController else { return }
            self?.seen.remove(ObjectIdentifier(controller))
            log("controller disconnected: \(controller.vendorName ?? "?")")
        }
        GCController.controllers().forEach(attach)
    }

    var connectedNames: [String] {
        GCController.controllers().map { $0.vendorName ?? "?" }
    }

    private func attach(_ controller: GCController) {
        guard seen.insert(ObjectIdentifier(controller)).inserted else { return }
        log("controller connected: \(controller.vendorName ?? "?")")
        guard let pad = controller.extendedGamepad else { return }
        let dpad = pad.dpad
        pad.leftThumbstick.valueChangedHandler = { [weak self] _, x, y in
            self?.leftStick = CGPoint(x: CGFloat(x), y: CGFloat(y))
        }
        pad.rightThumbstick.valueChangedHandler = { [weak self] _, x, y in
            self?.rightStick = CGPoint(x: CGFloat(x), y: CGFloat(y))
        }
        controller.physicalInputProfile.valueDidChangeHandler = { [weak self] _, element in
            guard let self else { return }
            if let button = element as? GCControllerButtonInput {
                // Sub-buttons of the d-pad and sticks belong to a collection;
                // the d-pad branch below handles the d-pad.
                guard button.collection == nil else { return }
                self.report(Self.name(of: element), button.isPressed)
            } else if let pad = element as? GCControllerDirectionPad, pad === dpad {
                self.report("Up", pad.up.isPressed)
                self.report("Down", pad.down.isPressed)
                self.report("Left", pad.left.isPressed)
                self.report("Right", pad.right.isPressed)
            }
        }
    }

    /// Reports only changes, so a held button does not repeat.
    private func report(_ name: String, _ pressed: Bool) {
        let changed = pressed ? down.insert(name).inserted : down.remove(name) != nil
        if changed { onButton(name, pressed) }
    }

    private static func name(of element: GCControllerElement) -> String {
        let name = (element.localizedName ?? "?").replacingOccurrences(of: " Button", with: "")
        // macOS names the stick buttons "Left Stick Button" on a Switch Pro
        // Controller and "Left Thumbstick Button" on others.
        switch name {
        case "Left Thumbstick", "Left Stick": return "L3"
        case "Right Thumbstick", "Right Stick": return "R3"
        default: return name
        }
    }
}
