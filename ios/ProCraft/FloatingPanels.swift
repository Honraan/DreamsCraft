import SwiftUI

// The Layers panel and the brush library torn off into windows of their own, while a plugin lets
// them (the floating_panels hook): each dragged out by a grabber at its top, as the colour panel
// is, then a window over the canvas that stays open while one draws, moved by the strip at its
// top, sized from its lower right corner and closed by its Close (its button then hangs it from
// the bar again). Inside, the panel is the same as where it hangs; the window is kept where it was
// left. Where a window may go, how big it may be and how it is kept are the core's (floating.rs).

/// The strip a floating panel's window adds above the panel: its grabber, and Close at its right.
private let floatingTopHeight: CGFloat = 24
/// How far down a floating window's grabber is (at the window's own size): what is held when it
/// is dragged out.
private let floatingGrabY: CGFloat = 12
/// The strip a floating panel's window adds under the panel, where its corner sizes it from.
private let floatingBottomHeight: CGFloat = 20
/// How far a panel's grabber is dragged before the panel floats, as the colour panel's.
private let panelUndockDrag: CGFloat = 30
/// What the strips over and under a floating panel take from its window's height.
let floatingStrips = floatingTopHeight + floatingBottomHeight

/// The floating panels' windows: each as it was last left (shown or closed), kept with the app's
/// settings, and where they may go. A window shows while a plugin lets its panel float.
@Observable
final class FloatingPanels {
    /// The panels that may float, in the order their windows are drawn.
    static let panels: [FloatingPanel] = [.layers, .brushes]

    /// Each panel's window as last left.
    private(set) var windows: [FloatingPanel: FloatingWindow] = [:]
    /// A panel's grabber is being dragged out: the panel hanging stays, unseen, until let go, its
    /// window following the finger.
    private(set) var undocking: FloatingPanel?
    /// Where the windows may go (interface points): the screen, clear of the side bar.
    private(set) var room = FloatingRoom(left: 0, top: 0, right: 1, bottom: 1)

    init() {
        for panel in Self.panels {
            if let kept = UserDefaults.standard.string(forKey: floatingKey(panel: panel)), let window = floatingRead(kept: kept) {
                windows[panel] = window
            }
        }
    }

    /// Whether a plugin lets `panel` float now.
    func allowed(_ panel: FloatingPanel) -> Bool {
        _ = PluginsModel.shared.changes
        return PluginsModel.shared.store.floats(panel: panel)
    }

    /// How big the windows show next to the rest of the interface, as a plugin says (1: as the rest).
    var scale: Float {
        Float(Look.shared.scale(.floating))
    }

    /// `panel`'s window while it shows (torn off, not closed, and a plugin lets it float), inside the
    /// room as it is now, which may have shrunk since it was left.
    func shown(_ panel: FloatingPanel) -> FloatingWindow? {
        guard let window = windows[panel], window.open, allowed(panel) else { return nil }
        return floatingFitted(panel: panel, window: window, room: room, scale: scale)
    }

    /// The screen the windows float over is now `room` (interface points).
    func setRoom(_ room: FloatingRoom) {
        if room != self.room {
            self.room = room
        }
    }

    /// `panel`'s grabber dragged out past its start, the finger `at` (interface points): its window
    /// follows it, as big as it was last left, its grabber under the finger.
    func undock(_ panel: FloatingPanel, at: CGPoint) {
        undocking = panel
        let k = scale
        let first = floatingAt(panel: panel, x: Float(at.x), y: Float(at.y - floatingGrabY * CGFloat(k)), scale: k)
        var sized = first
        if let was = windows[panel] {
            sized.x = Float(at.x) - was.width * k / 2
            sized.width = was.width
            sized.height = was.height
        }
        windows[panel] = floatingFitted(panel: panel, window: sized, room: room, scale: k)
    }

    /// `panel`'s grabber let go: whether it was being dragged out, so it floats where it is (kept),
    /// and the panel that hung from its button is to close.
    func undocked(_ panel: FloatingPanel) -> Bool {
        guard undocking == panel else { return false }
        undocking = nil
        keep(panel)
        return true
    }

    /// `panel`'s window moved by `by` (interface points), inside its room.
    func move(_ panel: FloatingPanel, by: CGSize) {
        guard let window = windows[panel] else { return }
        windows[panel] = floatingMoved(panel: panel, window: window, dx: Float(by.width), dy: Float(by.height), room: room, scale: scale)
    }

    /// `panel`'s window sized from its corner, dragged by `by` (interface points), within what it may be.
    func resize(_ panel: FloatingPanel, by: CGSize) {
        guard let window = windows[panel] else { return }
        windows[panel] = floatingResized(panel: panel, window: window, dx: Float(by.width), dy: Float(by.height), room: room, scale: scale)
    }

    /// `panel`'s window closed by its Close: where it was and its size kept for when it floats again.
    func close(_ panel: FloatingPanel) {
        guard var window = windows[panel] else { return }
        window.open = false
        windows[panel] = window
        keep(panel)
    }

    /// `panel`'s window as it is now, kept for the next start.
    func keep(_ panel: FloatingPanel) {
        guard let window = windows[panel] else { return }
        UserDefaults.standard.set(floatingKept(window: window), forKey: floatingKey(panel: panel))
    }
}

/// What a panel's window adds to it while it floats: how wide it shows the panel, and what its
/// grabber strip (`onMove`, by a drag in interface points, then `onLetGo`), its corner (`onResize`,
/// likewise) and its Close (`onClose`) do.
struct FloatingFrame {
    let width: CGFloat
    let height: CGFloat
    let onMove: (CGSize) -> Void
    let onResize: (CGSize) -> Void
    let onLetGo: () -> Void
    let onClose: () -> Void
}

/// A drag in the window's points, told as the step since the last change in the interface's points.
private struct StepDrag: ViewModifier {
    let onStep: (CGSize) -> Void
    let onEnd: () -> Void
    @State private var last = CGSize.zero

    func body(content: Content) -> some View {
        content.gesture(
            DragGesture(minimumDistance: 1, coordinateSpace: .global)
                .onChanged { drag in
                    let k = Interface.shared.scale
                    let step = CGSize(width: (drag.translation.width - last.width) / k, height: (drag.translation.height - last.height) / k)
                    last = drag.translation
                    onStep(step)
                }
                .onEnded { _ in
                    last = .zero
                    onEnd()
                }
        )
    }
}

/// The strip over a floating panel: its grabber in the middle (the whole strip drags the window),
/// Close at its right.
struct FloatingTopStrip: View {
    let frame: FloatingFrame

    var body: some View {
        ZStack(alignment: .top) {
            Color.clear
                .contentShape(Rectangle())
                .modifier(StepDrag(onStep: frame.onMove, onEnd: frame.onLetGo))
            Capsule()
                .fill(Pc.grabber)
                .frame(width: 40, height: 4)
                .padding(.top, 10)
                .allowsHitTesting(false)
            FloatingPanelClose(onClose: frame.onClose)
                .padding(.trailing, 4)
                .offset(y: -8)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .frame(height: floatingTopHeight)
        .accessibilityElement(children: .contain)
    }
}

/// The room a floating panel's window leaves under the panel, for its corner (`FloatingCorner`).
struct FloatingBottomStrip: View {
    var body: some View {
        Color.clear.frame(height: floatingBottomHeight)
    }
}

/// Close: a disc with a cross cut out of it, as the floating colour window's.
private struct FloatingPanelClose: View {
    let onClose: () -> Void

    var body: some View {
        Canvas { context, size in
            let o = CGPoint(x: size.width / 2, y: size.height / 2)
            let r: CGFloat = 8.5
            context.fill(Path(ellipseIn: CGRect(x: o.x - r, y: o.y - r, width: r * 2, height: r * 2)), with: .color(.white.opacity(0.15)))
            let d: CGFloat = 3.2
            var cross = Path()
            cross.move(to: CGPoint(x: o.x - d, y: o.y - d))
            cross.addLine(to: CGPoint(x: o.x + d, y: o.y + d))
            cross.move(to: CGPoint(x: o.x + d, y: o.y - d))
            cross.addLine(to: CGPoint(x: o.x - d, y: o.y + d))
            context.stroke(cross, with: .color(.black.opacity(0.3)), lineWidth: 1.3)
        }
        .frame(width: 40, height: 40)
        .contentShape(Rectangle())
        .onTapGesture(perform: onClose)
        .accessibilityLabel("Close")
        .accessibilityAddTraits(.isButton)
    }
}

/// A floating panel's lower right corner, two short lines across it: dragged, it sizes the window.
private struct FloatingCorner: View {
    let frame: FloatingFrame

    var body: some View {
        Canvas { context, size in
            // two lines across the corner, the nearer shorter
            let end = CGPoint(x: size.width - 12, y: size.height - 6)
            for reach: CGFloat in [10, 5] {
                var line = Path()
                line.move(to: CGPoint(x: end.x - reach, y: end.y))
                line.addLine(to: CGPoint(x: end.x, y: end.y - reach))
                context.stroke(line, with: .color(Pc.grabber), style: StrokeStyle(lineWidth: 2, lineCap: .round))
            }
        }
        .frame(width: 36, height: floatingBottomHeight)
        .padding(.trailing, 6)
        .contentShape(Rectangle())
        .modifier(StepDrag(onStep: frame.onResize, onEnd: frame.onLetGo))
        .accessibilityLabel("Resize")
    }
}

/// The grabber at the top of a panel hanging from its button, while a plugin lets it float: dragged
/// out past `panelUndockDrag`, the panel goes unseen and its window follows the finger; let go, it
/// floats there.
struct PanelGrabber: View {
    let model: CanvasModel
    let panel: FloatingPanel

    var body: some View {
        ZStack(alignment: .top) {
            Capsule()
                .fill(Pc.grabber)
                .frame(width: 40, height: 4)
                .padding(.top, 4)
                .allowsHitTesting(false)
            // (narrower than the colour panel's: clear of a title beside it)
            Color.clear
                .frame(width: 100, height: 30)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 1, coordinateSpace: .global)
                        .onChanged { drag in
                            let floating = model.floating
                            guard floating.undocking == panel || hypot(drag.translation.width, drag.translation.height) > panelUndockDrag else { return }
                            floating.undock(panel, at: Interface.shared.point(drag.location))
                        }
                        .onEnded { _ in
                            if model.floating.undocked(panel) {
                                model.closePanels()
                            }
                        }
                )
        }
        .frame(maxWidth: .infinity, alignment: .top)
    }
}

/// The panels floating in windows of their own, each where it was left, at the floating windows'
/// size: over the canvas (which takes no touch under them), under the panels hanging from the bar.
/// `screen` is the screen in the interface's points; they keep clear of the side bar as the panels
/// do.
struct FloatingPanelWindows: View {
    let model: CanvasModel
    let screen: CGSize

    private var room: FloatingRoom {
        let clear = model.showSideBar ? Pc.sideBarShown + Pc.popoverGap : 0
        let (w, h) = (Float(screen.width), Float(screen.height))
        return AppSettings.shared.rightHanded
            ? FloatingRoom(left: 0, top: 0, right: w - Float(clear), bottom: h)
            : FloatingRoom(left: Float(clear), top: 0, right: w, bottom: h)
    }

    var body: some View {
        let floating = model.floating
        ZStack(alignment: .topLeading) {
            ForEach(FloatingPanels.panels, id: \.self) { panel in
                if let window = floating.shown(panel) {
                    FloatingPanelWindow(model: model, panel: panel, window: window, scale: CGFloat(floating.scale))
                }
            }
        }
        .frame(width: screen.width, height: screen.height, alignment: .topLeading)
        .onChange(of: room, initial: true) { _, room in floating.setRoom(room) }
    }
}

/// One panel's window: laid out at its own size, shown `scale` times as big, at its place.
private struct FloatingPanelWindow: View {
    let model: CanvasModel
    let panel: FloatingPanel
    let window: FloatingWindow
    let scale: CGFloat

    var body: some View {
        let floating = model.floating
        let frame = FloatingFrame(
            width: CGFloat(window.width),
            height: CGFloat(window.height),
            onMove: { floating.move(panel, by: $0) },
            onResize: { floating.resize(panel, by: $0) },
            onLetGo: { floating.keep(panel) },
            onClose: { floating.close(panel) }
        )
        ZStack(alignment: .bottomTrailing) {
            switch panel {
            case .layers:
                LayersPanel(model: model, arrowX: 0, maxHeight: frame.height, floating: frame)
                    // what shows when it opens: the layers' thumbnails
                    .onAppear { model.refreshThumbnails() }
            case .brushes:
                BrushLibrary(model: model, arrowX: 0, origin: .zero, screen: CGSize(width: frame.width, height: frame.height), floating: frame)
            }
            FloatingCorner(frame: frame)
        }
        .frame(width: frame.width, height: frame.height, alignment: .top)
        // the canvas under it takes no touch
        .contentShape(Rectangle())
        .onTapGesture {}
        .scaleEffect(scale, anchor: .topLeading)
        .offset(x: CGFloat(window.x), y: CGFloat(window.y))
    }
}
