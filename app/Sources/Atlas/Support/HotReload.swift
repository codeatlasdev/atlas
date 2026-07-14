import SwiftUI

#if DEBUG
/// Enables InjectionIII hot reload in debug builds.
/// Install InjectionIII from: https://github.com/johnno1962/InjectionIII
///
/// Usage in any view:
///   @ObserveInjection var injection
///   var body: some View { ... .enableInjection() }
class InjectionObserver: ObservableObject {
    @Published private(set) var injectionCount = 0

    init() {
        NotificationCenter.default.addObserver(
            self, selector: #selector(injected),
            name: Notification.Name("INJECTION_BUNDLE_NOTIFICATION"), object: nil
        )
    }

    @objc private func injected() {
        injectionCount += 1
    }
}

extension View {
    func enableInjection() -> some View {
        return self
    }
}

@propertyWrapper
struct ObserveInjection: DynamicProperty {
    @ObservedObject private var observer = InjectionObserver()
    var wrappedValue: Int { observer.injectionCount }
}
#endif
