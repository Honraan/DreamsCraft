import Foundation

/// Runs `body` on a thread of its own with an 8 MB stack and returns what it made, the caller
/// waiting for it. Making, opening or bringing in an artwork goes deep into the core, deeper than
/// the 512 KB a dispatch queue's or a task's thread has; called from a queue, the queue's order
/// (one artwork opened at a time, imports one after another) holds as before.
func onBigStack<T>(qos: QualityOfService = .userInitiated, _ body: @escaping () -> T) -> T {
    let run = BigStackRun(body)
    let done = DispatchSemaphore(value: 0)
    let thread = Thread {
        run.made = run.body()
        done.signal()
    }
    thread.stackSize = 8 << 20
    thread.qualityOfService = qos
    thread.start()
    done.wait()
    // set before the signal
    return run.made!
}

/// What `onBigStack` hands its thread and gets back: touched by one thread at a time, the caller
/// waiting while the thread runs.
private final class BigStackRun<T>: @unchecked Sendable {
    let body: () -> T
    var made: T?

    init(_ body: @escaping () -> T) {
        self.body = body
    }
}
