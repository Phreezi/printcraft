# PeDeeFe: lista de melhorias

A lista de pedidos para a próxima ronda. Plano para 4.ª feira, 14/10, a partir das 14h00:
1. juntar o código original (PdfCraft, storytold/pdfcraft), que mudou muito, com as nossas
   alterações; onde ambos fizeram o mesmo, ficar com a melhor versão para o que o utilizador
   precisa;
2. corrigir os pontos desta lista;
3. enviar e publicar uma versão de teste, para experimentar a atualização automática
   (Preferências › Atualizações) a partir da test.14.

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

2. **Documento 1905.pdf (orçamento N.F. Pegado) mal mostrado.** No PeDeeFe só aparecem o logótipo
   e as caixas vazias; faltam todo o texto e o código QR. Noutros leitores aparece bem.
   - O ficheiro (enviado a 13/10) é do utilizador e tem dados reais: usar só para verificar,
     **nunca o pôr no repositório**; fazer um ficheiro de teste sintético com as mesmas
     características.
   - O que o ficheiro tem: gerado por QuickReports; letras Arial e Arial,Bold TrueType **não
     incorporadas**, com codificação própria (Custom); imagens em linha (inline) de 383×100
     (logótipo, aparece) e 130×130 (QR, não aparece).
   - Pistas: substituir letras não incorporadas por uma letra do sistema/embutida com as mesmas
     larguras (como fazem os outros leitores); ver porque falha a imagem em linha do QR.
   - Ver primeiro se o PdfCraft original já corrigiu isto (está a mudar de renderizador).

3. **Aviso de atualizações em cada abertura.** Hoje a verificação automática corre no máximo uma
   vez por dia e só mostra um aviso passageiro. Pedido:
   - sempre que o PeDeeFe abre, verifica em segundo plano se há versão nova, sem atrasar a
     abertura nem gastar recursos (um único pedido pequeno ao GitHub, numa tarefa à parte, com
     limite de tempo; sem rede, falha em silêncio);
   - se houver versão nova, mostra uma janela (pop-up) a dizer que versão é, com "Transferir e
     instalar", "Mais tarde" e "Não voltar a avisar";
   - "Mais tarde" (ou fechar a janela): volta a aparecer em cada abertura até atualizar;
   - "Não voltar a avisar": deixa de aparecer. A decidir: só para essa versão (volta a avisar
     quando sair outra) ou de vez (como desligar "Procurar atualizações automaticamente" nas
     Preferências). Proposta: só para essa versão, porque desligar de vez já existe nas
     Preferências;
   - a janela não aparece por cima de uma impressão rápida (`--print`) nem de outra janela aberta;
     espera que ela feche.

## Pendentes de rondas anteriores

- **Impressão Rápida do Outlook:** à espera do teu teste com a test.14 e o PeDeeFe como app
  predefinida para `.pdf`. Hipótese: um botão nas Preferências "Tornar o PeDeeFe a aplicação
  predefinida para PDF", que abre essa página das Definições do Windows.
- **Por enviar para o GitHub:** Ctrl+Tab / Ctrl+Shift+Tab entre separadores e botão do meio
  para fechar um separador (commit bb6cdc8). Só envio quando disseres.
