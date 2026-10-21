#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint atlas_mobile_native.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'atlas_mobile_native'
  s.version          = '0.0.1'
  s.summary          = 'iroh P2P transport for Atlas Mobile.'
  s.description      = <<-DESC
iroh P2P transport for Atlas Mobile, bound to Flutter via flutter_rust_bridge.
                       DESC
  s.homepage         = 'https://github.com/codeatlasdev/atlas'
  s.license          = { :type => 'MIT' }
  s.author           = { 'Atlas' => 'team@codeatlas.dev' }

  # This will ensure the source files in Classes/ are included in the native
  # builds of apps using this FFI plugin. Podspec does not support relative
  # paths, so Classes contains a forwarder C file that relatively imports
  # `../src/*` so that the C sources can be shared among all target platforms.
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '15.0'

  s.script_phase = {
    :name => 'Build Rust library',
    # First argument is relative path to the `rust` folder, second is name of rust library
    :script => 'sh "$PODS_TARGET_SRCROOT/../cargokit/build_pod.sh" ../../native atlas_mobile_native',
    :execution_position => :before_compile,
    :input_files => ['${BUILT_PRODUCTS_DIR}/cargokit_phony'],
    # Let XCode know that the static library referenced in -force_load below is
    # created by this build step.
    :output_files => ["${BUILT_PRODUCTS_DIR}/libatlas_mobile_native.a"],
  }
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    # Flutter.framework does not contain a i386 slice.
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    # iroh (netwatch/netdev/portmapper) links Apple networking frameworks.
    'OTHER_LDFLAGS' => '-force_load ${BUILT_PRODUCTS_DIR}/libatlas_mobile_native.a -framework Network -framework SystemConfiguration -framework Security -framework CoreFoundation',
  }
end