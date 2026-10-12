# PeDeeFe: lista de melhorias

A lista de pedidos para a próxima ronda. Na próxima sessão começamos por juntar o código original
(PdfCraft, storytold/pdfcraft) com as nossas alterações e depois tratamos estes pontos.

## Por fazer

1. **Abrir um documento no topo da 1.ª página.** Ao abrir um documento (talvez sempre, ainda por
   confirmar), a vista não fica no topo da primeira página:
   - com 2 ou mais páginas fica numa zona ao acaso (na captura de 12/10: um PDF de 3 páginas
     abriu a 139% no fim da página 1 / início da página 2, com a página 2 marcada no painel
     Páginas);
   - um ficheiro novo de 1 página abre no fim da página em vez de no topo.
   - Esperado: abre sempre no topo da página 1, como no Acrobat.
   - Pista: pode ter vindo da Página única / ajuste ao abrir (commit 0e4b451) ou do restauro da
     última posição de leitura; verificar com 1, 2 e 3 páginas, a vários zooms.

## Pendentes de rondas anteriores

- **Impressão Rápida do Outlook:** à espera do teu teste com a test.14 e o PeDeeFe como app
  predefinida para `.pdf`. Hipótese: um botão nas Preferências "Tornar o PeDeeFe a aplicação
  predefinida para PDF", que abre essa página das Definições do Windows.
- **Por enviar para o GitHub:** Ctrl+Tab / Ctrl+Shift+Tab entre separadores e botão do meio
  para fechar um separador (commit bb6cdc8). Só envio quando disseres.
