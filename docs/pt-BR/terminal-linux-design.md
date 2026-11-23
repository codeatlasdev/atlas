---
title: "Terminal GPU nativo no Linux: design"
source: terminal-linux-design.md
source_sha: "ba5cadbed3f8218784941bc06a11e8d90b2aeb98"
translation: machine
---

# Terminal GPU nativo no Linux: design

Acompanhamento: [#249](https://github.com/codeatlasdev/atlas/issues/249), fase Linux [#247](https://github.com/codeatlasdev/atlas/issues/247), liberação da GPU de terminais ocultos [#254](https://github.com/codeatlasdev/atlas/issues/254), setup de bench [#239](https://github.com/codeatlasdev/atlas/issues/239), engine [#252](https://github.com/codeatlasdev/atlas/issues/252) / [#253](https://github.com/codeatlasdev/atlas/issues/253) / [#255](https://github.com/codeatlasdev/atlas/issues/255). Roadmap [#289](https://github.com/codeatlasdev/atlas/issues/289). Contexto: [terminal.md](terminal.md) (o terminal do macOS e o roadmap de Windows/Linux) e [perf/linux-webkit-study.md](perf/linux-webkit-study.md) (onde um terminal no Linux gasta CPU hoje).

Isto é pesquisa, não uma implementação. Tudo o que está marcado como **medido** foi medido em uma máquina, em uma sessão (2026-11-23); tudo o que está marcado como **[HYPOTHESIS]** não foi. O mantenedor decide a partir da seção 7.

## 1. Resposta em dez linhas

1. **O Wayland não exige migração para o GTK4.** Um `wl_subsurface` da janela GDK Wayland, posicionado *abaixo* de uma página WebKitGTK transparente, carrega hoje uma superfície wgpu/Vulkan (RADV) do `atlas-term`, no stack distribuído tao 0.37 + wry 0.57 + GTK 3. Menus e tooltips em HTML aparecem sobre o terminal porque a página fica acima dele. **Medido, funciona** (seção 4.1).
2. **A janela filha X11 (o plano da #247) tem de verdade o problema de airspace**: uma janela filha nativa cobre todos os overlays HTML, e não há como fazer a página desenhar sobre ela (seção 4.2). Ela também exige `GDK_BACKEND=x11` para o app inteiro em uma sessão Wayland.
3. **O GTK4/webkitgtk-6.0 não é necessário para o Wayland e não está pronto**: o port GTK4 do wry ainda é uma issue aberta (#1474) com branches não integradas, o PR do Tauri está aberto, e o GTK4 abandona o libappindicator (a bandeja do Atlas). O `webkitgtk-6.0` 2.54.1 existe no Arch (seção 4.3).
4. **Um terminal no processo web (WebGPU/WASM) não é viável como caminho rápido**: `navigator.gpu` é `undefined` no WebKitGTK 2.54.1; o WebGL2 funciona; o núcleo VT compila para WASM a ~0,8x da velocidade nativa. Ele mantém todo o pipeline pty → hostd → app → IPC → página (seção 4.4).
5. **O terminal não é lento só por causa do parsing.** `Vt::feed` faz 88-168 MB/s nesta máquina (meta em docs/terminal.md: 500). O xterm.js com escritas grandes fica na mesma faixa (75 MB/s de ASCII). O que custa 170 % de CPU hoje é o transporte e a parte da página nele (seção 3).
6. **A escrita na grade é o gargalo de `Vt::feed`, não o parser VT nem os scanners do atlas**: o vte sozinho faz o parsing de ASCII a 3,3 GB/s, o `Term` do alacritty a 95 MB/s, o `Vt::feed` a 88. Um perfil de pilha de 250 amostras põe ~35 % do tempo de ASCII/`seq` no drop de `Option<Arc<CellExtra>>` ao sobrescrever células (seção 5.2).
7. **O libghostty-vt é 1,2-8,8x mais rápido em 6 de 8 corpora e 0,58x em truecolor com estilos únicos** (seção 5.3). O crate Rust dele exige o Zig **0.15.2**, não o 0.17 que a #255 menciona (o 0.17.1 falha no build).
8. **As leituras do pty no Linux são de 4 KiB** qualquer que seja o buffer passado e, com o processamento de saída ligado em um flood de `seq`, ficaram em ~160 B (907.000 leituras para 143 MB): o custo por leitura (três varreduras e dois locks por `Vt::feed`) pesa mais do que o benchmark de 64 KiB sugere (seção 5.4).
9. **Da tecla ao frame apresentado: p50 0,8 ms, p95 2,2 ms** no caminho do subsurface (79 teclas; termina na chamada de present do wgpu, não nos fótons). Ocioso: 0,2 % de um núcleo com um relógio de um segundo rodando.
10. **Recomendação**: ficar no GTK3; Wayland via wl_subsurface + wgpu Vulkan com uma página transparente e o terminal abaixo; manter o xterm.js para sessões X11, NVIDIA proprietária e qualquer falha; corrigir a engine (escrita na grade, #252/#253) em paralelo, porque isso também compensa no macOS (seção 6).

## 2. Método e limites

Máquina: Ryzen 7 7735HS (16 threads), 5,5 GB de RAM compartilhados com outros agentes (load1 1,0-2,9 nas execuções tranquilas, 5-6 nas primeiras; cada tabela diz qual), Radeon 680M, Mesa 26.2.5 (RADV `REMBRANDT`, radeonsi), WebKitGTK 2.54.1, GTK 3.24.52, sway 1.12, Linux 7.2.15. Rust 1.99.0, profile release com `debug = 1`. Compositor privado invisível: `WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128` (a GPU real), D-Bus privado com `--config-file` e sem servicedirs, `WAYLAND_DISPLAY`/`DISPLAY` removidos para o daemon, entrada só por `wtype`/`grim` no seat privado. O `hyprctl clients` listou 1 janela antes e depois.

O que **não** foi feito, para que nada abaixo o afirme: o Hyprland real do usuário (o empilhamento de subsurfaces e o `wp_fractional_scale` foram verificados só no sway), NVIDIA, Intel, sessões X11 além do Xwayland, IME (o caminho de preedit está só esboçado na página do spike), acessibilidade, vários terminais ou splits em uma janela, um filme de redimensionamento por arrasto da janela (um redimensionamento foi exercitado e o slot acompanhou, mas não foi filmado a 60 fps), renderização dentro do processo web (o spike WASM mediu só o parser) e builds release do próprio Atlas Desktop (as linhas de xterm.js/IPC citam o `docs/perf/terminal-binary-channel.md` já commitado).

O código do spike fica em `~/.cache/GpuTermResearch` e não está commitado: `vtbench` (benchmarks de parser sobre `atlas_term::vt::Vt`), `ghbench` (libghostty-vt), `subspike` (GTK3 + WebKitGTK + wl_subsurface + wgpu; variantes `x11` e `tw` = tao + wry), `vtwasm`, uma página de teste do xterm.js e um gerador de corpora que reaproveita o `frameBytes` de `scripts/term-agent-gen.ts`, de modo que os padrões de agente são os do próprio bench. O `atlas-term` foi copiado e alterado só na árvore de rascunho: `Renderer::new_raw(RawDisplayHandle, RawWindowHandle, …)` no lugar do construtor com `NSView`, features `vulkan`+`gles` do wgpu, e o alpha mode escolhido a partir das capacidades da superfície.

Corpora (120x40, em `~/.cache/GpuTermResearch/corpus`): `seq` (linhas `1..N`, 64 MiB), `ascii` (linhas de prosa de 96 colunas, 64 MiB), `sgr` (8 trechos coloridos por linha, 32 MiB), `repaint256` (um repaint de tela cheia, fg+bg de 256 cores em todas as células com CUP por linha dentro de 2026, o formato do zed#61819, 16 MiB), `truecolor` (o mesmo com um primeiro plano de 24 bits aleatório por célula: muitos estilos únicos, o "caminho lento conhecido") e os três padrões de agente `ink-2026`, `ratatui-diff`, `omp-boxes` de `term-agent-gen.ts` (16 MiB cada).

## 3. Onde o tempo vai hoje (xterm.js no Linux)

De `docs/perf/terminal-binary-channel.md` (release, loop de flood `yes … | head -c 20M`, janela de 30 s, display com GPU, depois da #734): **170 %** de um núcleo no total (app 58,7, web 85,1, hostd 26,4), processo web com 307 MB de PSS; llvmpipe 197 %. Só o processo web já é cerca de um núcleo: ele faz o parsing com o xterm.js e desenha com o addon WebGL.

xterm.js 6.0.0 + `@xterm/addon-webgl` 0.19.0 na própria página, em uma janela GTK3 WebKitGTK simples (sem Tauri, sem IPC; o corpus é escrito direto em `term.write`), 100x30, mesma máquina, load 0,6-1,0. "acked" = blocos de 64 KiB, cada um esperando o callback de `write` (o padrão que o `ptyStream.ts` usa); "bulk" = blocos de 1 MiB, com um callback a cada quatro:

| corpus | acked: MB/s | acked: CPU do processo web por MB | bulk: MB/s | bulk: CPU web por MB |
|---|---|---|---|---|
| ascii (64 MiB) | 8,1 | 48 ms | 75 | 17 ms |
| seq (64 MiB) | 7,2 | 98 ms | 17 | 66 ms |
| sgr (32 MiB) | 8,0 | 64 ms | n/m | n/m |
| truecolor (16 MiB) | 8,3 | 39 ms | 57 | 8 ms |
| ink-2026 (16 MiB) | 8,3 | 42 ms | n/m | n/m |

(n/m = não medido.) A taxa "acked" fica perto de 8 MB/s em todas as linhas, qualquer que seja o conteúdo, então ela é definida pelo `await` por bloco do callback de `write` (uma ida e volta por 64 KiB), não pelo parser [HYPOTHESIS: sem profiling]; e mostra por que a política de blocos e de ack da página (`ptyStream.ts`, lotes de 64 KiB ou 500 ms) decide a vazão. As linhas "bulk" são o teto de parser+renderer: os 75 MB/s de ASCII **não são mais lentos que o `Vt::feed` nativo (88)**. O `seq` é o ponto fora da curva (17 MB/s): a saída de uma linha por linha rola a grade a cada linha, e o caminho WebGL do xterm.js paga isso na etapa de render. Com o addon WebGL removido (renderer DOM), a mesma execução de ASCII deu 7,8 MB/s em acked e 5,3 s de CPU web contra 3,2 s com WebGL.

Leitura: um terminal nativo elimina o salto hostd → app → canal → página e a segunda cópia do trabalho no processo web, mas **não** ganha por fazer o parsing de ASCII mais rápido que o xterm.js. Ele ganha onde o xterm.js é estruturalmente fraco: saída com muita rolagem, memória, muitos terminais e a thread principal da página. Não prometa 500 MB/s com a troca de transporte; isso é uma questão da engine (seção 5).

## 4. Opções de apresentação

### 4.1 Wayland: wl_subsurface da janela GDK (spike 1, **medido, recomendado**)

Como: `gdk_wayland_window_get_wl_surface` (o `wl_surface` da toplevel) e `gdk_wayland_display_get_wl_display`, ambos da `libgdk-3`, encapsulados com o `from_foreign_display` do `wayland-backend` e o `wayland-client`; vincular `wl_compositor` + `wl_subcompositor` + `wp_viewporter` + `wp_fractional_scale_manager_v1`; `create_surface`, `get_subsurface(child, parent)`, `set_desync()`, `place_below(parent)`, uma região de entrada vazia (para que o ponteiro e as teclas fiquem com a webview, que é dona do foco, da seleção e do IME); `wgpu::Instance::create_surface_unsafe` com `RawWindowHandle::Wayland` (superfície filha) e `RawDisplayHandle::Wayland` (o display do GDK). A página informa o `getBoundingClientRect()` do seu slot (um `ResizeObserver` em um `<div>` transparente) e o Rust chama `wl_subsurface.set_position` mais `wp_viewport.set_destination` com ele; o swapchain tem o tamanho do slot em pixels físicos.

O que o spike provou (sway gles2 no renderD128, janela GTK3, página WebKitGTK transparente, um `sh` imprimindo `date` a cada segundo por meio do `Vt`):

* **Renderiza**: uma superfície Vulkan RADV de 1020x764 (`PreMultiplied`/`Opaque`, `Fifo`) sob a página; criação do renderer em 18-77 ms com instância aquecida (uma partida a frio mediu 65 ms).
* **Overlays**: com o terminal *abaixo* da página, um `div` HTML (z-index 10, um menu opaco) aberto sobre o terminal **é desenhado sobre ele** (screenshot comparado com a ordem *acima*, em que o terminal cobria o menu). Colocar o subsurface *acima* da página reproduz exatamente o problema de airspace; portanto, *abaixo + uma região transparente da página* é a ordem que mantém menus, tooltips, a paleta de comandos e diálogos funcionando sem lógica de ocultação.
* **O stack distribuído funciona**, não só uma janela GTK feita à mão: o mesmo código em `tao::WindowBuilder` + `wry::WebViewBuilder::with_transparent(true).build_gtk(window.default_vbox())` (a variante `tw`) produziu o resultado idêntico, usando `tao::platform::unix::WindowExtUnix::gtk_window()` para chegar ao `GdkWindow`.
* **Página transparente**: `window.set_visual(rgba_visual)`, `set_app_paintable(true)`, `webview.set_background_color(0,0,0,0)` (o wry faz as três coisas com `transparent: true`). O slot da página é `transparent`; tudo ao redor é opaco. O estudo mediu uma janela transparente como **sem custo de CPU no sway** (`linux-webkit-study.md`, Knobs); a regra `transparent_on` do Linux em `main.rs` diz "inferred, not measured" e precisaria ser ativada para a janela principal.
* **Escala 2 e 1,5**: com o `wp_fractional_scale_v1` vinculado à superfície da **toplevel** (`preferred_scale` = 180 para 1,5, ou seja, 1,5) o swapchain tem 1,5x o slot lógico e os glifos ficam nítidos (recorte comparado com o buffer de escala inteira 2 da webview). Vinculado à superfície **filha**, o evento nunca chega (a superfície não tem buffer nem output ainda): medidos 1.000 ms de espera e um fallback para o `scale_factor()` inteiro do GTK (2) nas duas execuções, o que desenha no tamanho errado em 1,5. Portanto, a fonte da escala é a toplevel.
* **Redimensionamento**: a mensagem do slot acompanha o layout (`ResizeObserver`), a grade é reajustada a partir do tamanho em pixels do slot, e o `resize set` do sway para 800x500 e de volta para 1100x780 produziu frames corretos. Não foi filmado.
* **Memória**: o processo do spike (GTK + processo de UI do WebKitGTK + dispositivo wgpu + terminal) 76 MB de PSS ocioso sem terminal, 75-106 MB com um terminal e um flood; o processo web do WebKit 64-102 MB; a memória de GPU não pôde ser atribuída (os `mem_info_*` do sistema variaram ±100 MB com outros agentes rodando, então nenhum número de memória de GPU é afirmado).

Custos e problemas em aberto (todos **[HYPOTHESIS]**, salvo indicação):

* **Recorte e rolagem.** Um subsurface não respeita `overflow`, `border-radius`, `clip-path` do CSS nem um ancestral com rolagem; um terminal dentro de um painel com rolagem, atrás de um cabeçalho fixo ou sob um modal arredondado precisa ser ocultado ou redimensionado pelo Rust. A página deve informar a visibilidade (um `IntersectionObserver` mais um sinal de "um overlay cobre o slot") como o `Placement` do macOS faz.
* **Sincronização de frames com a página.** O `set_desync()` deixa o terminal fazer commit no próprio relógio; durante um redimensionamento, a página e o swapchain podem divergir por um frame (visível como uma lacuna ou uma faixa de sobreposição). Candidatos de correção: `set_sync()` com commits na thread principal durante mudanças de layout, ou desenhar o fundo do slot na página.
* **Compositores.** Verificado no wlroots (sway). O Hyprland não usa wlroots como base e nada aqui rodou nele; GNOME/Mutter e KDE/KWin precisam de uma execução cada. Específico do Hyprland: nenhum conhecido.
* **Sessões XWayland e X11** não têm este caminho (seção 4.2).
* **O `wl_display` é compartilhado com o GDK**: o código de superfície Wayland do wgpu deve usar a conexão do GDK (usou, pelo raw handle); é necessária uma segunda `wl_event_queue` para os nossos objetos, como a `registry_queue_init` do `wayland-client` fornece.
* **Entrada e IME**: com uma região de entrada vazia, a webview mantém ponteiro, teclado e IME. A página do spike usou um `<textarea>` oculto e focado (o modelo do xterm.js), então composição de IME, teclas mortas e a repetição de teclas do compositor ficam com a página. O caminho de teclas medido abaixo passou por um handler de key-press do GTK. Os dois chegam ao pty; a escolha é a mesma que o `termKeys.ts` faz no macOS.

Latência (79 teclas do `wtype` com 120 ms de intervalo para um pty com `cat`; carimbo no `key-press-event` do GTK, parada quando o próximo `Renderer::render` retorna `Ok(true)` com o eco dessa tecla já na grade; isso termina na chamada de present do wgpu, **não nos fótons**, então some até um refresh): **p50 0,8 ms, p95 2,2 ms, máx 4,7 ms**. O equivalente no xterm.js (keydown → callback de `term.write` → próximo `requestAnimationFrame`, 79 teclas, eco local sem pty) foi **p50 10 ms, p95 17 ms, máx 17 ms**; ele termina em um limite de frame, então os dois números não são a mesma grandeza; eles delimitam "o nativo apresenta no mesmo milissegundo; a página apresenta no próximo frame". Um valor real de tecla ao fóton precisa de um timestamp de frame do compositor (o sway não tem um em modo headless) e é um item da fase 0.

Custo de render (tempo de parede de `Renderer::render` incluindo a espera de `get_current_texture`, flood de 3 passadas): p50 de 0,48-0,82 ms em ASCII/`seq`/`sgr`, p50 de 3,4-3,6 ms nos corpora de repaint (toda célula muda, 33 frames por 25 MB). Os frames por segundo ficaram no piso de 16 ms (`FRAME_INTERVAL`).

### 4.2 Janela filha X11 / XWayland (plano da #247; spike 2, **medido, airspace confirmado**)

Como: `GDK_BACKEND=x11` no XWayland do sway privado (`DISPLAY=:2`), um `GtkOverlay` com a webview como filha e um overlay `GtkDrawingArea`; o `GdkWindow` dele tornado nativo (`has_native() == true`, XID `0x400011`), `gdk_x11_window_get_xid`, e `RawWindowHandle::Xlib` + `RawDisplayHandle::Xlib` para o wgpu.

Resultados: o wgpu Vulkan sobre uma superfície Xlib sob o XWayland **renderiza** (as capacidades ofereceram só os alpha modes `PreMultiplied` e `Inherit`, não `Opaque`: o `Renderer::new` precisa escolher a partir de `caps.alpha_modes`, ou o configure falha com um erro de validação, como aconteceu no início). O menu HTML aberto no mesmo slot que em 4.1 **não ficou visível**: a janela X nativa é composta sobre todo o desenho da toplevel GTK, então qualquer menu, tooltip, diálogo, paleta de comandos, toast ou dropdown que se sobreponha a um terminal fica oculto por ela. Não existe ordem de empilhamento no X11 que ponha pixels da página sobre uma janela filha da mesma toplevel; as opções são a abordagem do macOS (ocultar ou encolher o terminal enquanto um overlay está aberto, com um flash), ou o WebKit renderizando em uma superfície separada (não disponível).

Outros custos: forçar `GDK_BACKEND=x11` faz a janela *inteira* do Atlas virar um cliente XWayland em uma sessão Wayland (borrada em escala fracionária, sem input method Wayland, sem gestos nativos); em uma sessão X11 de verdade nada disso se aplica. A execução com escala 2 esperou 1.005 ms por um evento de escala fracionária que não pode existir no X11.

Veredito: o X11 é o único lugar onde o design de janela filha se aplica, ele tem o problema de airspace sem solução, e as sessões X11 são uma fatia em queda. Mantenha o xterm.js ali até que uma base de usuários prove o contrário; não construa primeiro o caminho XCB/Xlib.

### 4.3 GTK4 / webkitgtk-6.0 (pesquisa, nada medido)

* **Status (lido em 2026-11-23).** A [wry#1474](https://github.com/tauri-apps/wry/issues/1474) está aberta desde janeiro de 2025; os mantenedores escreveram que "no real work has been done on the working group side". Existem branches da comunidade (`Ranrar/wry` `gtk4-webkit6`, `gtk4` 0.11 + `webkit6` 0.6, testado em Wayland/Ubuntu 26.04 e em uma VM X11; o port do tao por `johncarmack1984` [tao#1258](https://github.com/tauri-apps/tao/pull/1258), draft; uma branch GTK4 do muda; [tauri#14684](https://github.com/tauri-apps/tauri/pull/14684), aberto). Um app Tauri rodou nesse stack em um teste de um contribuidor. O port do wry lista mudanças incompatíveis: um `WebViewHandle` opaco, `with_related_view`, `HardwareAccelerationPolicy` movido, e o embutimento de filhas por `RawWindowHandle::Xlib` está **quebrado por design** no GTK4 (use `build_gtk(container)`). Um mantenedor pediu uma feature flag GTK3+GTK4 em julho de 2026 e foi voto vencido; não há plano de release.
* **O que custaria ao Atlas.** Substituir `gtk` 0.18, `webkit2gtk` 2.0, tao, wry, muda, tray-icon e todo chamador de `with_webview` (`linux_gfx.rs`, o tratamento de crash do `webkit`, a linha de título, a bandeja). **O GTK4 abandona o libappindicator**, então a bandeja passa para o ksni (StatusNotifierItem sobre D-Bus); a bandeja do Atlas (`tray.rs`) e a dependência obrigatória de `libayatana-appindicator` do pacote Arch (#419) mudam. Mudanças de empacotamento em `tauri.linux.conf.json`, na linha apt do CI, no bundling do AppImage e na matriz de distros (`verify-linux`). O `webkitgtk-6.0` 2.54.1 está no repositório do Arch (38 MB de download); o Ubuntu 24.04 e o Debian stable trazem pacotes WebKitGTK 6 mais antigos [INFERENCE: not checked per distro].
* **O que traria ao terminal.** O modelo atual do Ghostty (issue #14052, integrada no Ghostty 1.4, ago-set 2026): o renderer desenha fora da thread de UI e exporta cada frame como um DMA-BUF; um `GtkWidget` personalizado o transforma em um `GdkDmabufTexture` no `snapshot()`. Não é o `GtkGLArea` (contexto GL na thread principal; o Ghostty o abandonou). A thread de issue do Ghostty documenta os custos: os drivers NVIDIA não têm o caminho usual de exportação EGL (foi preciso um fallback com cópia pela CPU), lag por falta de sincronização de GPU entre a thread de render e o contexto do GTK, desenhos extras do triple-buffering.
* **wgpu e DMA-BUF.** O `wgpu-hal` 30.0.1 (a versão que o `atlas-term` fixa) tem `vulkan::Device::texture_from_dmabuf_fd` (somente **importação**), protegido por `Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF`, que o RADV desta máquina **expõe** (medido com `enumerate_adapters`: `true` no adaptador Vulkan, `false` no GL). **Não há exportação**; o "needs raw Vulkan (`VK_EXT_image_drm_format_modifier` + `VK_KHR_external_memory_fd`)" da #247 está confirmado para o lado da exportação. Um produtor de DMA-BUF que usa o wgpu precisa, portanto, de chamadas `ash` via `Device::as_hal` para alocar uma imagem exportável, `vkGetMemoryFdKHR`, mais o modifier DRM, e um objeto de sincronização para o GTK. Esse é um spike maior que o 4.1 e **não foi feito**.
* **O ponto decisivo**: nada em 4.1 precisa do GTK4. O GTK4 também expõe o `wl_surface` da toplevel (`gdk_wayland_surface_get_wl_surface`), então um `wl_subsurface` abaixo da página deve funcionar ali também **[HYPOTHESIS: GTK4's renderer may not leave a transparent hole for a subsurface placed below; not tested]**. O GTK4 daria uma integração melhor (uma textura que o GTK compõe e recorta, `GdkDmabufTexture`, `GtkGraphicsOffload` só como otimização, sem recorte nem overlays), mas é uma dependência em escala de ano do trabalho de outras pessoas.

### 4.4 Renderização no processo web (WebGL2 / WebGPU / WASM) (spike 4, **medido: só o parser**)

* **O WebGPU não está disponível**: `navigator.gpu` é `undefined` nesta janela WebKitGTK 2.54.1 (os destaques do 2.54 não listam nenhuma linha de WebGPU; o compositor Skia e o damage tracking são as novidades gráficas).
* **O WebGL2 está disponível** (`getParameter(UNMASKED_RENDERER_WEBGL)` informa a string do próprio engine, `Apple GPU`, não o dispositivo real). O addon do xterm já o usa.
* **O WASM com SIMD funciona** (`WebAssembly.validate` em um módulo SIMD: true). O `SharedArrayBuffer` é `undefined` (sem isolamento cross-origin); um worker precisaria de headers COOP/COEP no esquema próprio do app **[HYPOTHESIS: Tauri can set them; not checked]**.
* **Parser em WASM**: o `alacritty_terminal` 0.26 compilado para `wasm32-unknown-unknown` (uma cópia vendorizada sem os módulos de tty/event-loop, um `Timeout` sem relógio; 156 KB) e alimentado com blocos de 64 KiB na página, melhor de 3, MB/s: ascii 80, seq 92, sgr 94, truecolor 162, ink-2026 154; com `+simd128` 82, 75, 91, 168, 171. O mesmo `Term` nativamente (seção 5.1): 95, 118, 114, 197, 188. Portanto, **o WASM fica em ~0,8x do nativo, e o SIMD não faz nada por um parser limitado por escritas na grade**.
* **Não medido**: um renderer WebGL2 de terminal próprio, e o custo de levar os bytes do hostd para a página (o canal existente).

Veredito: a opção (d) mantém o salto de IPC e a thread principal da página, depende do compositor do WebKit para os pixels e não pode usar a GPU fora do processo web. O único mérito dela é que overlays, recorte e rolagem são exatamente os da página. É o *formato de fallback* certo se a engine passar para o Rust (o mesmo código de `Vt` e de glifos poderia dirigir um `OffscreenCanvas`), e o caminho primário errado.

### 4.5 Tabela de opções

| | A. wl_subsurface + wgpu (4.1) | B. janela filha X11 (4.2) | C. GTK4 + widget DMA-BUF (4.3) | D. processo web WASM/WebGL2 (4.4) | E. xterm.js hoje |
|---|---|---|---|---|---|
| Funciona em | sessões Wayland, GTK3 (stack distribuído) | X11 ou XWayland | Wayland e X11, depois do port do wry | em qualquer lugar onde o WebKit roda | em todo lugar |
| Medido aqui | renderiza, overlays, escala 1/1,5/2, teclas, flood, ocioso | renderiza; overlay oculto | importação de DMA-BUF do wgpu-hal disponível; nada além | só o parser (0,8x do nativo) | 75 MB/s de ASCII na página; 170 % de flood no app |
| Overlays HTML | funcionam (terminal abaixo, página transparente) | **cobertos, sem solução** | funcionam (o GTK compõe) | funcionam | funcionam |
| Recorte/rolagem na página | o Rust precisa recortar/ocultar | o Rust precisa recortar/ocultar | o GTK recorta | CSS | CSS |
| Tecla → present | 0,8 / 2,2 ms (p50/p95, termina no present) | não medido | não medido | frame da página (~10 / 17 ms até o rAF) | ~10 / 17 ms até o rAF |
| Novas dependências | wayland-client, wayland-protocols, wayland-backend; wgpu `vulkan` (+`gles`) | um handle Xlib/XCB (o wgpu tem); `GDK_BACKEND=x11` | gtk4, webkit6, ports de tao/wry/muda/tray-icon, ksni; mais exportação Vulkan crua | nenhuma | nenhuma |
| Bloqueado por | nada | o problema de airspace | wry#1474, tao#1258, tauri#14684 | sem caminho de GPU fora do WebGL2 | nada |
| Risco | diferenças entre compositores, recorte, NVIDIA/DMA-BUF do WebKit (nota de risco da #247) | XWayland borrado, UX de ocultar durante overlay | o maior | o ganho é pequeno | o status quo |

## 5. O núcleo VT (spike 3)

Tudo aqui é `feed` single-thread de um corpus que já está em memória, 120x40, melhor de 5 / mediana, MB/s, profile release, load 1,5-1,7 (`vt-final-64k.txt`, `gh-final-64k.txt`; as execuções anteriores, com mais carga, concordaram dentro de ±10 %).

### 5.1 Camadas, mesmos corpora

| corpus | parser `vte`, performer no-op | `alacritty_terminal::Term` (parser + grade) | `atlas_term::Vt::feed` | `libghostty-vt` 0.2.2 (`vt_write`, ReleaseFast) |
|---|---|---|---|---|
| seq | 3.426 | 118 | 106 | 167 |
| ascii | 3.297 | 95 | **88** | 771 |
| sgr | 485 | 114 | 94 | 144 |
| repaint256 | 337 | 203 | 150 | 178 |
| truecolor | 319 | 197 | 143 | **83** |
| ink-2026 | 950 | 188 | 168 | 577 |
| ratatui-diff | 902 | 222 | 174 | 765 |
| omp-boxes | 807 | 227 | 169 | 220 |

* **`Vt::feed` vs `Term`**: os scanners do atlas (`ClearScan`, `SeqScan` para OSC 133/7/9/777, APC, sixel; `PixelScan` para DECSET 1016) e seus locks custam 7-30 %: 10 % em `seq`, 7 % em ASCII, 18 % em sgr, 26 % em repaint256, 27 % em truecolor, 11 % em ink, 22 % em ratatui, 26 % em omp. **Este é o teto da #252** (passada única): até ~25 % em tráfego com formato de repaint, cerca de 7 % em texto, de acordo com os 5-10 % medidos no Windows. Blocos de 4.095 bytes (leituras de pty do Linux) mudaram o `Vt::feed` em menos de 5 % (seq 104, ascii 88, sgr 93, truecolor 140, ink 158).
* **Snapshot** (`Vt::snapshot` de uma tela truecolor completa de 120x40 em um `Frame` reaproveitado): 86-137 µs conforme a carga (88,7 µs na execução tranquila); o percurso do render-state do libghostty-vt na mesma tela (`graphemes_len` + fg + bg por célula, sem montar um `Frame`): 61 µs. Não é um gargalo a 60 Hz (0,5 % de um frame).
* A medição anterior de 70-105 MB/s em um notebook Windows está na mesma faixa destas (88-174) nesta CPU. Os "cerca de 170 MB/s otimizado" do docs/terminal.md para o M4 Pro são da mesma ordem. **A meta de 500 MB/s está fora de alcance com a grade do `alacritty_terminal`** em qualquer uma das máquinas medidas: o limite é a escrita na grade, não o parser.

### 5.2 Onde o `Vt::feed` gasta o tempo (amostragem de pilha)

O `perf` não está instalado (`kernel.perf_event_paranoid=2`, sem binário `perf`). O `gdb` está: um amostrador artesanal parou o `vtbench` (só `Vt::feed`, 40 passadas) a cada ~40 ms e registrou o frame do topo, 250 amostras por corpus (frames inlined são atribuídos à função que os contém, então leia isto como um ranking, não como percentuais exatos):

| frame do topo | seq | ascii |
|---|---|---|
| `drop_glue<Option<Arc<CellExtra>>>` | **84** (34 %) | **91** (36 %) |
| `Row::index_mut` | 16 | 27 |
| `Term::input` / `write_at_cursor` | 29 | 34 |
| `InternalBitFlags::intersects` | 15 | 13 |
| `Storage::compute_index` | 8 | 10 |
| `Row::reset` / `Cell::reset` | 8 | 11 |
| `scan.rs` / `pixel.rs` `position::<u8>` (scanners do atlas) | 14 (6 %) | 16 (6 %) |
| `vte` `ground_dispatch` / `print` | 13 | 15 |

A execução truecolor produziu poucas amostras para ranquear (a execução é curta); os frames do topo foram `vte::Parser::advance` e `advance_csi_param`: o parsing de SGR com cinco parâmetros por célula, como esperado.

**Causa (o ranking é medido; o mecanismo é lido do tipo, não provado por um contador)**: escrever um caractere em uma célula na grade do alacritty faz o drop do `extra: Option<Arc<CellExtra>>` antigo da célula (caracteres de largura zero, hyperlink, cor de sublinhado) mesmo quando ele é `None`, por meio de um drop glue genérico chamado para cada célula escrita ou resetada, e `Row::reset`/a rolagem resetam cada célula de cada linha rolada. O layout de célula do libghostty-vt (estilos em tabelas por página, células sem ponteiros próprios) evita isso [INFERENCE from its documentation and speed, its source was not read]. Os scanners são 6 % em texto, o que concorda com a diferença de 7 % (ASCII) entre `Term` e `Vt::feed` em 5.1.

O que isso significa para o plano: um `Vt::feed` mais rápido neste núcleo é uma mudança de representação da grade (o `Cell` do alacritty tem 24 bytes com o `Arc`), isto é, um patch upstream ou um fork do `alacritty_terminal`, ou a troca do núcleo (5.3). A passada única da #252 vale ≤ 7 % em texto e ≤ 27 % em repaints.

### 5.3 libghostty-vt (#255) no Linux

* **Build.** O `libghostty-vt` 0.2.2 / `libghostty-vt-sys` 0.2.2 fazem o build com `zig build` pelo `build.rs` (ele busca o Ghostty no commit fixado `a887df42…` e exige o **Zig 0.15.2**: o Zig **0.17.1 falha** com "Your Zig version v0.17.1 does not meet the required build version of v0.15.2"; o "Zig 0.17" da #255 está errado para esta versão do crate). **Uma armadilha que custou 10 minutos**: o crate escolhe o modo de otimização do Zig a partir do `DEBUG` do cargo; um profile com `debug = 1` (como o profile de bench do `atlas-term` e o profile release usado pelos builds com `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS` do desktop) compila o libghostty em silêncio no modo `Debug` do Zig (o `seq` de ASCII não terminou uma passada de 64 MiB em minutos). Defina `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`. Os números acima são ReleaseFast. Tempo de build aqui: 55-95 s com 4 jobs (`zig build` da biblioteca estática + bindings).
* **Velocidade.** Contra o `Vt::feed`: ascii **8,8x**, ratatui-diff 4,4x, ink-2026 3,4x, seq 1,6x, sgr 1,5x, omp-boxes 1,3x, repaint256 1,2x, **truecolor 0,58x**. Essa última linha é o caso de "muitos estilos únicos" que o mantenedor do Ghostty chama de caminho lento conhecido, e é o formato dos redraws de tela cheia de agentes com cor por célula. Os padrões de agente (ink, ratatui, omp) são 1,3-4,4x mais rápidos, porque repintam texto quase idêntico. A afirmação do próprio Ghostty de 725 MB/s de ASCII sintético está a 6 % dos 771 medidos.
* **Encaixe da API (lida, não integrada ao Atlas).** `Terminal`, `RenderState`, `RowIterator`, `CellIterator` são `!Send + !Sync` ("drive all operations from a single thread"); `begin_update`/`end` é o snapshot em duas fases, amigável a locks. `frame.rs`, `pick.rs`, `find.rs`, a seleção, `images.rs` e `search.rs` do Atlas são escritos contra o `Term`/`Grid` do alacritty; hyperlinks, marcas OSC 133, sixel e o protocolo kitty graphics (a feature `kitty-graphics` do crate) teriam de ser religados. Lacunas a verificar, nenhuma verificada: entrega de OSC 7/9/777, DECSET 1016, a retenção de 1 s da atualização sincronizada (`vt/sync.rs`; a do próprio Ghostty é de 1 s), orçamento de bytes do scrollback, semântica de limpar com histórico.
* **Custo de adotar.** Zig 0.15.2 no CI em três SOs (Windows e macOS não testados), um fetch git fixado no `build.rs` (rede em tempo de build: o Atlas compila em sandboxes e para o empacotamento do Arch; as features `pkg-config` e `link-dynamic` existem para uma biblioteca pré-compilada), e um crate jovem (0.2.x, "pre-1.0").
* **Veredito**: **adiar, com uma decisão a tomar**: é o único caminho para ≥ 3x em tráfego de TUI de agentes sem escrever a nossa própria grade, mas perde no repaint de estilos únicos e é uma reescrita multiplataforma de ~3.000 linhas do `atlas-term`. Rode o spike completo da #255 (parse + snapshot por um adaptador que preenche o `Frame`) antes de decidir, e faça primeiro a correção mais barata da escrita na grade (abaixo).

### 5.4 Tamanhos de leitura do pty e #253

Um `cat` do corpus para um master de pty neste kernel (processamento de saída desligado, `os.read(fd, 64 KiB)`; `ptysz.py` na árvore de rascunho): **ASCII 67 MB em 0,27 s = 249 MB/s, 12.209 leituras, média de 5.497 B, 86 % das leituras de exatamente 4.095 B**; truecolor igual (média 5.453, 87 % de 4.095); um buffer de 1 MiB não mudou nada (média 4.467). O pty do Linux entrega um buffer tty de 4 KiB por vez; a leitura de 64 KiB em `lifecycle.rs::spawn_reader` não é uma leitura de 64 KiB.

Com a configuração real do spike (processamento de saída ligado, `cat` escrevendo no slave do pty, thread leitora → `Vt::feed` → thread de render, `PASSES=2-3`): ASCII 187 MB em ~2,75 s = **68 MB/s**, 28.968 leituras (6,4 KB cada), thread leitora com 2,59 s de CPU (13,9 ms/MB); `seq` 143 MB a **34 MB/s**, **907.476 leituras (157 B cada)**, 22,2 ms/MB; `sgr` 52 MB a 69 MB/s. A thread de render usou 0,14 s para 181 frames. **Portanto, o caso `seq` é limitado pelo overhead por leitura, não pelo parsing** (as escritas do cat pela disciplina de linha do tty chegam em pedaços minúsculos; o leitor do Atlas pega o primeiro bloco disponível e faz três passadas de varredura e dois mutexes por chamada). Medido aqui como fato; a causa dos blocos minúsculos (disciplina de linha vs escalonamento) é **[HYPOTHESIS]**.

Consequências para a #253: (a) no Linux o lado produtor faz 250 MB/s com leituras de 4 KiB e o consumidor faz 90-170 MB/s, então o consumidor é o gargalo e uma thread de parse só sobrepõe a syscall de leitura (2,4 ms/MB de 11 ms/MB, ≤ 20 %); (b) **juntar leituras pequenas antes do `feed`** (um lock e uma passada de varredura por ~64 KiB ou por 1 ms) ajuda mais floods de blocos minúsculos do que a thread; (c) a primeira leitura curta ainda deve passar direto, para que a latência de digitação não mude.

## 6. Recomendação e plano em fases (os números de aceitação são propostas)

**Arquitetura**: manter o GTK3/webkit2gtk-4.1 e o tao/wry como distribuídos. As sessões Wayland do Linux ganham um terminal nativo por meio de um **wl_subsurface posicionado abaixo de uma região transparente da página**, renderizando com o renderer wgpu do `atlas-term` em **Vulkan** (RADV/ANV; o backend GL só como fallback de probe, atrás do watchdog existente). Sessões X11, XWayland (`GDK_BACKEND=x11`), NVIDIA proprietária e qualquer falha de adaptador continuam no xterm.js; o gate do frontend `gpuTerminalSupported()` passa a ser uma capacidade decidida pelo Rust (tipo de sessão, backend, adaptador) em vez de `platform === "macos"`. O trabalho de engine (escrita na grade, coalescência) é compartilhado com macOS e Windows e avança de forma independente.

Um PR por fase, cada um com números de aceitação medidos com o setup de bench da fase 0 nesta classe de máquina (Ryzen/iGPU Radeon, Wayland, load < 2):

| Fase | PR | O quê | Aceitação (números a superar/manter) |
|---|---|---|---|
| 0 | `test(bench): terminal corpora and parser bench on Linux` (linha Linux da #239) | commitar o gerador de corpora (reaproveitando o `term-agent-gen.ts`), um bench de parser `#[ignore]` no `atlas-term` (`seq`, `ascii`, `sgr`, `repaint256`, `truecolor`, padrões de agente; melhor/mediana), uma sonda de tamanho de leitura do pty, `docs/perf/terminal-linux-baseline.json` com máquina/driver/build e um carimbo de tecla até o present para que a fase 3 tenha uma linha de base | tabela da seção 5.1 reproduzida dentro de ±10 %; linha do xterm.js na página da seção 3 reproduzida; método de timestamp de frame do `hyprctl`/compositor para tecla até o fóton decidido e documentado |
| 1 | `perf(term): grid write without per-cell drop` (engine, #252 incorporada) | remover o drop de `Option<Arc<CellExtra>>` do caminho quente (um patch/fork fino do `Cell`/`Row` do alacritty, ou um caminho rápido com `ManuallyDrop` quando `extra.is_none()`), uma passada de varredura sobre os bytes, `feed` recebe leituras inteiras | `Vt::feed` ≥ 150 MB/s em ASCII e ≥ 140 MB/s em `seq` (de 88 / 106), sem regressão em truecolor (143) ou sgr (94), todos os testes de `agent_frames` e `vt/tests.rs` inalterados; `cat` de 1 GiB no macOS não mais lento |
| 2 | `perf(term): coalesce reads, parse off the reader thread` (#253) | juntar leituras pequenas (4 KiB no Linux) em um `feed` por ~64 KiB ou 1 ms, depois uma thread de parse atrás de um ring limitado; a primeira leitura curta passa direto | flood de `seq` na configuração de spike da fase 0 ≥ 100 MB/s (de 34) e ASCII ≥ 120 MB/s (de 68); p95 do eco de tecla inalterado (≤ 1 frame + 2 ms) |
| 3 | `feat(desktop): Linux wl_subsurface terminal behind a capability` (#247, Wayland) | `term/linux.rs`: subsurface abaixo da página, superfície wgpu Vulkan, sincronização do slot, park/hide, escala fracionária a partir da superfície da toplevel, `Renderer::new_raw` recebendo raw handles, alpha mode a partir das caps, diretórios de fontes + fontconfig, divisão do `term_stub.rs` para o Linux ter comandos reais, janela principal transparente no Wayland | escala 1, 1,5, 2 nítidas e do tamanho certo; um menu, tooltip e a paleta desenhados sobre um terminal em execução (teste de screenshot no sway privado); redimensionamento a 60 fps sem frame de lacuna ou sobreposição (filme); tecla → present p95 ≤ 4 ms in-process; 4 TUIs de agente a 30 redraws/s < 5 % de CPU total (a meta da #239 é 3 %); terminal ocioso ≤ 0,5 % de um núcleo; flood de `terminal-output` app + web + hostd < 100 % (hoje 170) |
| 4 | `perf(term): release GPU of hidden terminals` (#254) | política em um módulo sem gate (plano do macOS de `batch3-plan.md`), depois descartar a superfície Vulkan, os buffers e os atlas dos terminais estacionados após T e reconstruir ao mostrar | 20 terminais, 1 visível: GPU/RSS antes e depois medidos com `Renderer::gpu_bytes()` + PSS; reconstrução ao mostrar < 16 ms sem flash visível (varredura de flicker, variante Linux) |
| 5 | `feat(term): IME, clipboard and a11y on Linux` | entrada de texto pelo elemento focado da página ou por `zwp_text_input_v3`; seleção primária; AT-SPI | composição de `fcitx5`/`ibus`, teclas mortas, ida e volta de copiar/colar, leitor de tela anuncia a saída: cada um como um teste com o produtor real |
| 6 | `spike(term): libghostty-vt behind vt/` (#255) | o spike completo com um adaptador de `Frame`, Zig 0.15.2 no CI, `LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast`, as lacunas de OSC/DECSET/sixel listadas | relatório de decisão com ≥ 2x nos padrões de agente e não pior que 0,8x em truecolor, ou descartar |

As fases 1, 2, 4 e 6 são independentes da 3 e também compensam no macOS; a fase 3 depende só da 0. Ativar por padrão exige as fases 3 e 5 e um release de opt-in (`Configurações → Terminal → Terminal rápido`, já `atlas.gpuTerminal`).

## 7. Decisões que só o mantenedor pode tomar

1. **Subsurface no GTK3 ou migração para o GTK4** no Wayland. Recomendação: GTK3 agora (está medido funcionando e pode ser entregue em semanas, e o port GTK4 não está integrado upstream); rever quando a wry#1474 for integrada. O código do subsurface é em boa parte reutilizável no GTK4 **[HYPOTHESIS]**.
2. **Estratégia de overlay**: *terminal abaixo de um slot transparente* (recomendado; os overlays funcionam como estão; custa uma janela principal transparente no Linux, que o estudo mediu como gratuita só no sway) contra *terminal acima e ocultar/encolher enquanto um overlay está aberto* (a abordagem do X11 e da view filha do macOS; piscadas, exige um registro de overlays na página).
3. **Backend do wgpu**: só Vulkan (recomendado: RADV/ANV funcionam; menor superfície de dependências) ou Vulkan com fallback GL (acrescenta o caminho GL/EGL medido como disponível aqui, uma matriz de testes maior) ou nenhum dos dois na NVIDIA proprietária (xterm.js). Também se o Linux distribui `vulkan` na mesma lista de features que o Metal (o `atlas-term` compila só `metal` hoje; testes que são pulados no Linux passariam a rodar).
4. **X11 e XWayland**: ficar no xterm.js (recomendado) ou construir depois o caminho de janela filha com ocultar-durante-overlay.
5. **Estratégia da engine**: patch/fork da grade do `alacritty_terminal` (fase 1) contra adotar o libghostty-vt (fase 6) contra os dois em ordem (recomendado). Adotar exige uma toolchain Zig e um fetch de rede em tempo de build em todo build (ou uma biblioteca vendorizada/pré-compilada), em três SOs.
6. **Quais números viram gates de CI**: o gate de ociosidade do Linux já roda no llvmpipe (`linux-idle-gate.sh`); um terminal GPU não pode ser gateado em um runner sem GPU. Recomendação: vazão do parser com gate no CI (só CPU, relativa a uma linha de base registrada), números de GPU e de latência locais, com máquina e driver registrados em `docs/perf/`.
7. **Valores-alvo**: o "parse ≥ 500 MB/s" do roadmap não é alcançável com o núcleo atual (≤ 170 MB/s aqui; libghostty-vt 770 em ASCII, 83 em repaints de estilos únicos). Confirmar ou substituir por pisos por corpus (a fase 0 os propõe).
8. **Dependências**: `wayland-client`, `wayland-protocols`, `wayland-backend` (client_system) para a fase 3, cada uma revisada conforme `docs/supply-chain.md`.

## 8. Saída bruta do spike

Benchmarks de parser (`vtbench`, blocos de 64 KiB, melhor/mediana de 5, load 1,6): `seq 106/102, ascii 88/88, sgr 94/93, repaint256 150/148, truecolor 143/142, ink-2026 168/153, ratatui-diff 174/172, omp-boxes 169/166`; `Term` sozinho `118/109, 95/94, 114/112, 203/202, 197/196, 188/186, 222/221, 227/213`; `vte` no-op `3426, 3297, 485, 337, 319, 950, 902, 807`; `libghostty-vt` `167/164, 771/756, 144/143, 178/177, 83/82, 577/572, 765/752, 220/219`; snapshot 85,6 µs; percurso do render-state do ghostty 61,2 µs.

Floods do spike de subsurface (`FLOOD=…`, 2-3 passadas do corpus, buffer de leitura de 64 KiB): ascii 187 MB, 68 MB/s, CPU própria 2,82 s (15,1 ms/MB), 28.968 leituras, 181 frames, render p50/p95/máx 716/2.879/5.761 µs; truecolor 25 MB em 0,25 s, 100 MB/s, 12,1 ms/MB, 33 frames, render p50 3,6 ms; ink-2026 25 MB, 104 MB/s, 11,4 ms/MB, 32 frames; seq 143 MB, 34 MB/s, 22,2 ms/MB, 907.476 leituras; sgr 52 MB, 69 MB/s, 13,9 ms/MB. Ocioso depois do flood (a página e um loop de `date` de 1 s rodando): 0,20 % de um núcleo ao longo de 10 s, em todas as execuções.

xterm.js 6.0.0 + addon-webgl 0.19.0 na página (seção 3): acked: ascii 8,14 MB/s (CPU web 3,22 s de 67 MB), sgr 7,96 (2,16 s de 33,5 MB), truecolor 8,33 (0,65 s de 16,8 MB), ink-2026 8,33 (0,71 s de 16,8 MB), seq 7,15 (6,56 s de 67 MB); bulk de 1 MiB: ascii 75,0, seq 17,1, truecolor 56,8. PSS web 158-364 MB (seq o maior). Eco de tecla p50/p95/máx 10/17/17 ms (n = 79).

`alacritty_terminal` em WASM (melhor de 3, MB/s): nosimd seq 92, ascii 80, sgr 94, truecolor 162, ink-2026 154; simd seq 75, ascii 82, sgr 91, truecolor 168, ink-2026 171.

Latência de tecla do subsurface: n = 79, p50 0,8, p95 2,2, máx 4,7 ms. Escala fracionária: `preferred_scale` 1.500 milli na superfície da toplevel com escala de output 1,5 (swapchain de 1170x846 para um slot lógico de 780x564); a mesma requisição na superfície filha: nenhum evento em 1.005 ms, fallback 2.
