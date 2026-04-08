# Atlas Mobile Builds — Fastlane Integration Plan

## Objetivo

Adicionar suporte a builds mobile no Atlas CLI via Fastlane, permitindo que qualquer projeto (React Native, Flutter, nativo) compile, assine e distribua apps iOS/Android com um único comando.

```bash
atlas build ios --profile preview
atlas build android --profile production
atlas build all
```

---

## Por que Fastlane (e não EAS)

| Critério | EAS (Expo) | Fastlane + Atlas |
|---|---|---|
| Custo | Free tier limitado, $19-99/mês | Gratuito (open source) |
| Infra | Cloud (Expo servers) | Self-hosted (Mac local ou CI) |
| Lock-in | Expo ecosystem | Agnóstico (RN, Flutter, nativo) |
| Controle | Limitado | Total |
| Filosofia Atlas | Depende de terceiro | "Your own infra" ✓ |

---

## Arquitetura

```
atlas build ios
    │
    ├── 1. Detecta atlas.yaml → seção `mobile:`
    ├── 2. Resolve certificados (match ou manual)
    ├── 3. Executa Fastlane lane correspondente
    ├── 4. Upload (TestFlight / Play Store / Ad Hoc)
    └── 5. Notifica equipe (webhook / Slack / link direto)
```

### Componentes

1. **`atlas.yaml` — config mobile**
2. **`src/commands/build.ts`** — comando CLI
3. **`src/lib/fastlane.ts`** — wrapper Fastlane
4. **Fastfile template** — gerado pelo `atlas init` quando detecta projeto mobile

---

## Fase 1 — MVP (1-2 semanas)

### 1.1 Config no `atlas.yaml`

```yaml
name: ac-frota-driver
org: codeatlasdev

mobile:
  platform: react-native  # ou flutter, native-ios, native-android
  ios:
    bundle_id: br.com.automacaocuritiba.driver
    team_id: XXXXXXXXXX
    scheme: ACFrotaMotorista
    provisioning: match  # match (recomendado) ou manual
  android:
    package: br.com.automacaocuritiba.driver
    keystore: secrets/release.keystore  # gerenciado pelo atlas env

  profiles:
    development:
      ios: { export_method: development, distribution: internal }
      android: { build_type: debug }
    preview:
      ios: { export_method: ad-hoc, distribution: internal }
      android: { build_type: release, artifact: apk }
    production:
      ios: { export_method: app-store }
      android: { build_type: release, artifact: aab }
```

### 1.2 Comando `atlas build`

```typescript
// src/commands/build.ts
atlas build <platform> --profile <profile>

// Flags:
//   platform: ios | android | all
//   --profile: development | preview | production (default: preview)
//   --skip-upload: só compila, não distribui
//   --version: override version number
//   --changelog: release notes
```

### 1.3 Fastfile Template

Gerado automaticamente pelo `atlas init` ou `atlas build` na primeira execução:

```ruby
# ios/fastlane/Fastfile
default_platform(:ios)

platform :ios do
  desc "Build for internal testing (Ad Hoc)"
  lane :preview do
    setup_ci if ENV['CI']

    match(
      type: "adhoc",
      app_identifier: ENV["BUNDLE_ID"],
      readonly: is_ci
    )

    build_app(
      workspace: "#{ENV['SCHEME']}.xcworkspace",
      scheme: ENV["SCHEME"],
      export_method: "ad-hoc",
      output_directory: "./build",
      clean: true
    )
  end

  desc "Build for App Store"
  lane :production do
    setup_ci if ENV['CI']

    match(
      type: "appstore",
      app_identifier: ENV["BUNDLE_ID"],
      readonly: is_ci
    )

    build_app(
      workspace: "#{ENV['SCHEME']}.xcworkspace",
      scheme: ENV["SCHEME"],
      export_method: "app-store",
      output_directory: "./build",
      clean: true
    )

    upload_to_testflight(skip_waiting_for_build_processing: true)
  end
end
```

```ruby
# android/fastlane/Fastfile
default_platform(:android)

platform :android do
  lane :preview do
    gradle(
      task: "assemble",
      build_type: "Release",
      project_dir: "android"
    )
  end

  lane :production do
    gradle(
      task: "bundle",
      build_type: "Release",
      project_dir: "android"
    )

    upload_to_play_store(
      track: "internal",
      aab: "android/app/build/outputs/bundle/release/app-release.aab"
    )
  end
end
```

### 1.4 Fluxo do `atlas build ios --profile preview`

```
1. Lê atlas.yaml → mobile.ios + mobile.profiles.preview
2. Verifica dependências:
   - Fastlane instalado? (gem/brew)
   - Xcode instalado? (xcode-select)
   - CocoaPods instalado?
3. Se React Native: roda `npx expo prebuild` (se Expo) ou verifica ios/
4. Injeta env vars: BUNDLE_ID, SCHEME, TEAM_ID
5. Executa: `cd ios && bundle exec fastlane preview`
6. Coleta artefato (.ipa) de ./build/
7. Se --skip-upload não foi passado:
   - Ad Hoc: gera link de download (pode hospedar no próprio server Atlas)
   - TestFlight: upload via Fastlane pilot
8. Notifica (webhook configurável)
```

---

## Fase 2 — Certificados com Match (semana 3)

### Problema
Gerenciar certificados iOS é um inferno. Fastlane Match resolve isso guardando certificados e provisioning profiles num repo Git privado (ou storage S3/GCS).

### Implementação
- `atlas build setup-signing` — configura Match
- Cria repo privado `codeatlasdev/certificates` (ou usa S3)
- Match sincroniza certificados automaticamente nos builds
- Novos devs rodam `atlas build sync-certs` e tá pronto

```yaml
# atlas.yaml
mobile:
  ios:
    signing:
      method: match
      storage: git  # git | s3
      git_url: git@github.com:codeatlasdev/certificates.git
```

---

## Fase 3 — CI/CD Integration (semana 4)

### GitHub Actions Runner (Mac)

Para builds iOS no CI, precisa de macOS runner. Opções:

1. **Mac Mini dedicado** como self-hosted runner (~$600 one-time)
2. **GitHub Actions macOS** ($0.08/min, ~$1.50/build)
3. **MacStadium / AWS Mac** (cloud Mac, ~$50-100/mês)

### Workflow gerado pelo Atlas

```yaml
# .github/workflows/mobile-build.yml (gerado por atlas init)
name: Mobile Build
on:
  push:
    tags: ['v*']

jobs:
  build-ios:
    runs-on: macos-latest  # ou self-hosted Mac
    steps:
      - uses: actions/checkout@v4
      - uses: ruby/setup-ruby@v1
        with: { ruby-version: '3.2', bundler-cache: true }
      - run: atlas build ios --profile production --yes

  build-android:
    runs-on: ubuntu-latest  # Android não precisa de Mac
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-java@v4
        with: { distribution: 'temurin', java-version: '17' }
      - run: atlas build android --profile production --yes
```

---

## Fase 4 — Distribuição Self-Hosted (semana 5-6)

### OTA Updates (React Native)
- Integrar `expo-updates` ou `codepush` para updates over-the-air
- `atlas update` → envia JS bundle sem rebuild nativo
- Hosting no próprio server Atlas (S3 ou CDN)

### Ad Hoc Distribution Page
- Atlas Panel gera página de download para builds internos
- QR code para instalar no device
- Sem depender de TestFlight para testes internos
- Similar ao que o Diawi/AppCenter fazem, mas self-hosted

```bash
atlas build ios --profile preview
# → Build complete! Install: https://panel.codeatlas.com.br/builds/ac-frota-driver/42
```

---

## Dependências

| Ferramenta | Propósito | Instalação |
|---|---|---|
| Fastlane | Build + sign + distribute | `gem install fastlane` ou `brew install fastlane` |
| Match | Gerência de certificados | Incluso no Fastlane |
| Xcode | Compilação iOS | Mac App Store |
| Android SDK | Compilação Android | `sdkmanager` |
| CocoaPods | Deps iOS | `gem install cocoapods` |

---

## Prioridade de Implementação

```
Fase 1 (MVP)          ████████████░░░░  75% do valor
  - atlas build ios/android
  - Fastfile templates
  - Config no atlas.yaml

Fase 2 (Signing)      ██████░░░░░░░░░░  15% do valor
  - Match integration
  - Cert management

Fase 3 (CI/CD)        ███░░░░░░░░░░░░░  5% do valor
  - GitHub Actions workflow
  - Self-hosted runner

Fase 4 (Distribution) ██░░░░░░░░░░░░░░  5% do valor
  - OTA updates
  - Self-hosted install page
```

---

## Estrutura de Arquivos (no Atlas)

```
src/
├── commands/
│   └── build.ts              # atlas build <platform>
├── lib/
│   ├── fastlane.ts           # Fastlane runner + env injection
│   ├── mobile.ts             # Detect framework, parse config
│   └── signing.ts            # Match / cert management
└── templates/
    └── fastlane/
        ├── Fastfile.ios.rb    # Template iOS
        ├── Fastfile.android.rb # Template Android
        ├── Gemfile            # Ruby deps
        └── Matchfile          # Match config template
```

---

## Notas

- Fastlane é Ruby, mas o Atlas só invoca via shell — sem dependência Ruby no core
- Android builds rodam em Linux (CI barato), só iOS precisa de Mac
- Match elimina o problema de "quem tem o certificado" — fica centralizado
- A Fase 1 sozinha já substitui o EAS pra 90% dos casos de uso
