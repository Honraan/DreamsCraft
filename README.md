# DreamsCraft

Aplicativo de código aberto para desenhar e animar em 2D em tablets Android, Windows e Linux. O DreamsCraft usa o motor de pintura do [ProCraft](https://github.com/artcraft-android/procraft), com renderização por GPU em Rust e `wgpu` e interface em Kotlin e Jetpack Compose no Android. Desenhe quadro a quadro no Flipbook, organize clipes e áudio em uma linha do tempo multifaixa e grave movimentos em tempo real com o Perform.

## Recursos Principais (v2.0)

- **Timeline Multifaixa + Split View + Modo Flipbook:** Divisão dinâmica entre Palco (*Stage* com *Backstage*) e Timeline multifaixa, barra central arrastável, pinch-zoom horizontal/vertical e modo Flipbook em tela cheia.
- **Auto-Criação de Frame ao Desenhar (*Draw-to-Create*):** Desenhar em qualquer região vazia da timeline cria/estende automaticamente um frame/cel (e divide *holds* estendidos), com **Undo atômico em 1 passo**.
- **Modo Perform + Perform Motion Ghost:** Grave trajetórias de movimento, escala, rotação e **Warp** em tempo real com *Motion Filtering* (Ramer–Douglas–Peucker) e opção **Ghost 1st Frame** (onion skin do primeiro frame seguindo o movimento para animar quadro a quadro por cima).
- **Opções Completas de Track & Layer Effects em GPU:** Renomear, deletar, duplicar, reordenar, *Merge Down*, cores, modos de mesclagem, *Clip to Track Below*, *Alpha Lock*, *Mute* e efeitos não-destrutivos em WGSL (**Outline/Stroke**, **Drop Shadow**, **Outer/Inner Glow** e **Color Overlay**).
- **Trilhas de Áudio, Waveform, Audio Scrubbing & MP4 com Som:** Importação de áudio (WAV/MP3/AAC/OGG/FLAC), waveform na timeline, keyframes de Volume/Pan, reprodução sincronizada + *lip-sync scrubbing* via `AudioTrack` e exportação MP4/HEVC com áudio AAC.
- **Live Filters & Warp/Distort Animados:** Keyframes não-destrutivos de *Gaussian Blur*, *HSB*, *Sharpen*, *Noise*, *Distort* (4 cantos) e *Warp* (malha 4×4).
- **Multi-Camadas por Cel, VideoClip, TextClip, Camera Track Global e Gestos Stylus:** Sub-camadas por cel no Flipbook, importação de vídeo para rotoscopia, clipes de texto, câmera global (*Pan/Zoom/Rotate*) e seleção por Laço / *Scratch-to-Delete* na timeline.

## Executar localmente

No Windows ou Linux, com Rust instalado:

```sh
cargo run --release -p procraft-desktop
```

No Android, use as [instruções de compilação do ProCraft](README.procraft.md#building-instructions).

## Referências do Krita

O código do Krita em C++ e Qt serve **exclusivamente como referência arquitetural e conceitual** para o fluxo de animação do DreamsCraft (implementação 100% *clean-room* em Rust/Kotlin). Os links apontam para a revisão usada no clone de referência.

- [Linha do tempo, curvas e onion skin](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/plugins/dockers/animation)
- [Storyboard](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/plugins/dockers/storyboarddocker)
- [Renderização e exportação de animação](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/libs/ui/animation)

## Licença

A base do ProCraft é distribuída sob [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). Consulte a [documentação original](README.procraft.md#license), o [NOTICE](NOTICE) e as [atribuições](ATTRIBUTION.md) para os componentes de terceiros.
