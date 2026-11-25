import type { PreviewArea } from "./StylePreview";

export interface AreaInfo {
  title: string;
  /** Uma linha, no cartão do menu. */
  summary: string;
  description: string;
  /** O que a área muda de fato (referência antes de aplicar). */
  reference: string[];
}

export const AREA_INFO: Record<PreviewArea, AreaInfo> = {
  barra: {
    title: "Barra de tarefas",
    summary: "Alinhamento, busca, relógio e agrupamento dos botões.",
    description: "A faixa inferior do Windows, com o Iniciar, os apps abertos e o relógio.",
    reference: [
      "Alinhamento dos ícones: à esquerda (como no Windows 10) ou ao centro.",
      "Busca: oculta, só o ícone ou a caixa de pesquisa.",
      "Botões de Visão de Tarefas e Widgets, e segundos no relógio.",
      "Agrupar os botões: sempre, quando a barra encher ou nunca.",
    ],
  },
  iniciar: {
    title: "Menu Iniciar",
    summary: "Fixados, recomendações e pesquisa da web.",
    description: "O painel que abre com a tecla Windows.",
    reference: [
      "Mais itens fixados ou mais recomendações.",
      "Ocultar a seção Recomendados.",
      "Pesquisa do Iniciar sem resultados do Bing.",
    ],
  },
  explorador: {
    title: "Explorador de Arquivos",
    summary: "Menu de clique direito, extensões, ocultos e densidade.",
    description: "As janelas de pastas e arquivos.",
    reference: [
      "Menu de contexto clássico, sem o \"Mostrar mais opções\".",
      "Mostrar extensões e arquivos ocultos.",
      "Abrir em Este Computador e esconder Início e Galeria do painel lateral.",
      "Modo compacto, com linhas mais juntas.",
    ],
  },
  aparencia: {
    title: "Cores e aparência",
    summary: "Modo claro ou escuro, transparência e animações.",
    description: "O visual geral de janelas, apps e da barra do sistema.",
    reference: [
      "Modo escuro ou claro, separado para apps e para o sistema.",
      "Efeitos de transparência ligados ou desligados.",
      "Animações de janelas e da barra de tarefas.",
    ],
  },
  temas: {
    title: "Temas",
    summary: "Conjuntos de cores, papel de parede e terminal.",
    description: "Um tema junta cores, papel de parede e terminal; dá para exportar e importar.",
    reference: [
      "Aplicar um tema muda cores, papel de parede e terminal de uma vez.",
      "Dá para voltar ao visual anterior pelo Histórico.",
      "Exportar e importar arquivos .sentinela-theme.",
    ],
  },
  loja: {
    title: "Loja",
    summary: "Temas, ponteiros do mouse e pacotes de ícones para baixar.",
    description:
      "Um catálogo com prévia: escolha o que baixar, aplique de dentro do app e reverta quando quiser.",
    reference: [
      "Ponteiros do mouse (cursores) completos, incluindo os de espera animados.",
      "Pacotes de ícones para pasta, Este Computador, Lixeira, sua pasta e Rede.",
      "Temas que juntam cores, ponteiro e ícones; dá para desfazer tudo.",
      "Itens extras vêm do catálogo online, conferidos por SHA-256.",
    ],
  },
  wallpaper: {
    title: "Papel de parede",
    summary: "Galeria do Wallpaper Engine, por monitor.",
    description: "Escolha o papel de parede animado de cada monitor.",
    reference: [
      "Usa o Wallpaper Engine instalado neste computador.",
      "Um papel de parede por monitor.",
      "Pausa automática quando um jogo abre.",
    ],
  },
  terminal: {
    title: "Terminal",
    summary: "Oh My Posh, fonte e temas do prompt.",
    description: "O prompt do PowerShell e a aparência do Windows Terminal.",
    reference: [
      "Instala e configura o Oh My Posh.",
      "Fonte Nerd Font para os ícones do prompt.",
      "Temas do prompt com prévia.",
    ],
  },
  desktop: {
    title: "Desktop",
    summary: "Atalhos globais, regras de janela, barra, tiling e lançador.",
    description:
      "Recursos extras de desktop sem injetar nada em outros programas. A saída de emergência desfaz tudo.",
    reference: [
      "Atalhos globais e regras por janela (borda, opacidade, posição).",
      "Barra própria, tiling de janelas e lançador de apps, definidos no desktop.toml.",
      "Saída de emergência: Win+Ctrl+Shift+Esc.",
    ],
  },
  sistema: {
    title: "Sistema",
    summary: "Tela de bloqueio, entrada e encaixe de janelas.",
    description: "Comportamentos gerais do Windows.",
    reference: [
      "Desligar a tela de bloqueio e o desfoque da tela de entrada.",
      "Atalho das Teclas de Aderência.",
      "Encaixe de janelas nas bordas da tela.",
    ],
  },
};
