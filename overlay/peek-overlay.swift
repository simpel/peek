import Cocoa
import SwiftUI

// MARK: - Protocol Types

struct OverlayCommand: Codable {
    let action: String
    let items: [SuggestionItem]?
    let selected: Int?
    let cursorRow: Int?     // 1-based row in terminal
    let cursorCol: Int?     // 1-based col in terminal
    let termRows: Int?
    let termCols: Int?
}

struct SuggestionItem: Codable, Identifiable {
    let name: String
    let preview: String
    var id: String { name }
}

// MARK: - Overlay State

class OverlayState: ObservableObject {
    @Published var items: [SuggestionItem] = []
    @Published var selectedIndex: Int = 0
    @Published var isVisible: Bool = false
}

// MARK: - Dropdown View

struct DropdownView: View {
    @ObservedObject var state: OverlayState

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(state.items.prefix(8).enumerated()), id: \.element.id) { index, item in
                HStack(spacing: 0) {
                    Text(item.name)
                        .font(.system(size: 13, weight: index == state.selectedIndex ? .semibold : .regular, design: .monospaced))
                        .foregroundColor(index == state.selectedIndex ? .white : .primary)
                        .frame(maxWidth: 180, alignment: .leading)
                        .lineLimit(1)

                    Spacer(minLength: 12)

                    Text(item.preview)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(index == state.selectedIndex ? .white.opacity(0.7) : .secondary)
                        .frame(maxWidth: 220, alignment: .trailing)
                        .lineLimit(1)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    RoundedRectangle(cornerRadius: 5)
                        .fill(index == state.selectedIndex ? Color.accentColor : Color.clear)
                )
                .padding(.horizontal, 4)
            }
        }
        .padding(.vertical, 6)
        .frame(width: 440)
        .background(
            VisualEffectView(material: .hudWindow, blendingMode: .behindWindow)
                .clipShape(RoundedRectangle(cornerRadius: 10))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.primary.opacity(0.1), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.3), radius: 12, y: 4)
    }
}

// MARK: - NSVisualEffectView wrapper

struct VisualEffectView: NSViewRepresentable {
    let material: NSVisualEffectView.Material
    let blendingMode: NSVisualEffectView.BlendingMode

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}

// MARK: - Window Finder

/// Find the frontmost normal window that isn't our overlay.
/// CGWindowList returns windows in front-to-back order.
/// Window bounds are available WITHOUT Screen Recording permission.
func getFrontmostWindowFrame() -> CGRect? {
    let myPid = ProcessInfo.processInfo.processIdentifier

    guard let windowList = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements],
        kCGNullWindowID
    ) as? [[String: Any]] else {
        return nil
    }

    for window in windowList {
        guard let layer = window[kCGWindowLayer as String] as? Int,
              layer == 0,
              let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
              let pid = window[kCGWindowOwnerPID as String] as? Int32,
              pid != myPid
        else { continue }

        let w = bounds["Width"] ?? 0
        let h = bounds["Height"] ?? 0

        // Skip tiny windows (menu bar items, etc.)
        if w > 200 && h > 200 {
            return CGRect(
                x: bounds["X"] ?? 0,
                y: bounds["Y"] ?? 0,
                width: w,
                height: h
            )
        }
    }
    return nil
}

// MARK: - Overlay Window

class OverlayPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

class OverlayWindowController {
    let panel: OverlayPanel
    let state: OverlayState

    init() {
        state = OverlayState()

        panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: 440, height: 300),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: true
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = false

        let hosting = NSHostingView(rootView: DropdownView(state: state))
        hosting.frame = panel.contentView!.bounds
        hosting.autoresizingMask = [.width, .height]
        panel.contentView?.addSubview(hosting)
    }

    func show(items: [SuggestionItem], selected: Int, cursorRow: Int, cursorCol: Int, termRows: Int, termCols: Int) {
        DispatchQueue.main.async { [self] in
            state.items = items
            state.selectedIndex = selected

            let itemCount = min(items.count, 8)
            let itemHeight: CGFloat = 28
            let padding: CGFloat = 12
            let height = CGFloat(itemCount) * itemHeight + padding

            panel.setContentSize(NSSize(width: 440, height: height))

            // Position using frontmost window frame + cursor position
            if let winFrame = getFrontmostWindowFrame() {
                let screenHeight = NSScreen.main?.frame.height ?? 1080
                let titleBar: CGFloat = 28
                let contentH = winFrame.height - titleBar
                let cellH = contentH / CGFloat(termRows)
                let cellW = winFrame.width / CGFloat(termCols)

                // CG coordinates (top-left origin)
                let cursorBottomCG = winFrame.minY + titleBar + CGFloat(cursorRow) * cellH

                // Convert to NS coordinates (bottom-left origin)
                let nsY = screenHeight - cursorBottomCG - height
                let nsX = winFrame.minX + CGFloat(cursorCol - 1) * cellW

                // Clamp to screen
                let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1920, height: 1080)
                let clampedX = max(screen.minX, min(nsX, screen.maxX - 440))
                let clampedY = max(screen.minY, nsY)

                panel.setFrameOrigin(NSPoint(x: clampedX, y: clampedY))
            }

            panel.orderFrontRegardless()
            state.isVisible = true
        }
    }

    func updateSelection(_ index: Int) {
        DispatchQueue.main.async { [self] in
            state.selectedIndex = index
        }
    }

    func hide() {
        DispatchQueue.main.async { [self] in
            panel.orderOut(nil)
            state.isVisible = false
            state.items = []
        }
    }
}

// MARK: - Stdin Reader

class StdinReader {
    let controller: OverlayWindowController

    init(controller: OverlayWindowController) {
        self.controller = controller
    }

    func start() {
        DispatchQueue.global(qos: .userInteractive).async { [self] in
            let handle = FileHandle.standardInput
            var buffer = Data()

            while true {
                let data = handle.availableData
                if data.isEmpty {
                    DispatchQueue.main.async { NSApplication.shared.terminate(nil) }
                    break
                }

                buffer.append(data)

                while let newlineRange = buffer.range(of: Data([0x0a])) {
                    let lineData = buffer.subdata(in: buffer.startIndex..<newlineRange.lowerBound)
                    buffer.removeSubrange(buffer.startIndex...newlineRange.lowerBound)

                    if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                        processCommand(line)
                    }
                }
            }
        }
    }

    private func processCommand(_ json: String) {
        guard let data = json.data(using: .utf8),
              let cmd = try? JSONDecoder().decode(OverlayCommand.self, from: data) else { return }

        switch cmd.action {
        case "show":
            if let items = cmd.items {
                controller.show(
                    items: items,
                    selected: cmd.selected ?? 0,
                    cursorRow: cmd.cursorRow ?? 1,
                    cursorCol: cmd.cursorCol ?? 1,
                    termRows: cmd.termRows ?? 24,
                    termCols: cmd.termCols ?? 80
                )
            }
        case "update":
            if let sel = cmd.selected { controller.updateSelection(sel) }
        case "hide":
            controller.hide()
        default:
            break
        }
    }
}

// MARK: - App Delegate

class AppDelegate: NSObject, NSApplicationDelegate {
    var controller: OverlayWindowController!
    var reader: StdinReader!

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        controller = OverlayWindowController()
        reader = StdinReader(controller: controller)
        reader.start()

        // Hide when user switches away from terminal
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil, queue: .main
        ) { [weak self] _ in
            self?.controller.hide()
        }
    }
}

// MARK: - Main

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
