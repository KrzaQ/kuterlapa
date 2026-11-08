import Foundation
import GameController

/// Reads game controller buttons, also while another app is in front.
///
/// Button names are the labels on the controller, with " Button" removed:
/// A B X Y L R ZL ZR + - Share, and Up Down Left Right for the d-pad.
/// macOS keeps the Home button for itself, so it never arrives here.
final class Gamepad {
    private let onPress: (String) -> Void
    private var seen = Set<ObjectIdentifier>()

    init(onPress: @escaping (String) -> Void) {
        self.onPress = onPress
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
        let dpad = controller.extendedGamepad?.dpad
        controller.physicalInputProfile.valueDidChangeHandler = { [weak self] _, element in
            guard let self else { return }
            if let button = element as? GCControllerButtonInput {
                // Sub-buttons of the d-pad and sticks belong to a collection;
                // the d-pad branch below handles the d-pad.
                guard button.isPressed, button.collection == nil else { return }
                let name = (element.localizedName ?? "?").replacingOccurrences(of: " Button", with: "")
                self.onPress(name)
            } else if let pad = element as? GCControllerDirectionPad, pad === dpad {
                if pad.up.isPressed { self.onPress("Up") }
                else if pad.down.isPressed { self.onPress("Down") }
                else if pad.left.isPressed { self.onPress("Left") }
                else if pad.right.isPressed { self.onPress("Right") }
            }
        }
    }
}
