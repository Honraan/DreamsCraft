# DreamsCraft

Editor de animação 2D quadro a quadro, baseado no [ProCraft](https://github.com/artcraft-android/procraft). Novas telas em branco no desktop e no Android abrem com a linha do tempo ativa. A linha do tempo já permite criar e duplicar quadros, ajustar a duração, reproduzir a sequência e ver os quadros vizinhos com *onion skin*.

## Executar localmente

No Windows ou Linux, com Rust instalado:

```sh
cargo run --release -p procraft-desktop
```

No Android, use as [instruções de compilação do ProCraft](README.procraft.md#building-instructions).

## Referências do Krita

O código do Krita em C++ e Qt serve de referência para o fluxo de animação do DreamsCraft. Os links apontam para a revisão usada no clone de referência.

- [Linha do tempo, curvas e onion skin](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/plugins/dockers/animation)
- [Storyboard](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/plugins/dockers/storyboarddocker)
- [Renderização e exportação de animação](https://github.com/KDE/krita/tree/343e1773fc1ebfc2b711c0332dde37e2392a2d2c/libs/ui/animation)

## Licença

A base do ProCraft é distribuída sob [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). Consulte a [documentação original](README.procraft.md#license), o [NOTICE](NOTICE) e as [atribuições](ATTRIBUTION.md) para os componentes de terceiros.
