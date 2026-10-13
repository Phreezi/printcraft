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

4. **Imprimir: "Páginas" volta a "Todas" no documento seguinte.** Pedido de 13/10: imprimiu as
   páginas 1-2 de um documento e, no documento seguinte, a janela Imprimir continuava com
   "Páginas 1-2". Esperado: cada documento começa em **Todas**.
   - Causa: `open_print` (crates/ui-egui/src/print_ui.rs) guarda as escolhas da janela anterior
     (`..keep` do `print_draft`), incluindo `which` (Todas / Página atual / Páginas) e o intervalo.
   - Correção proposta: guardar a que documento pertence a escolha de páginas; noutro documento
     volta a Todas com o intervalo vazio. No mesmo documento mantém o que escolheu (como no
     Acrobat). Impressora, papel, qualidade e o resto continuam lembrados.
   - Ver também a mudança automática para "Página atual" ao escolher Janela (linha ~976): também
     não deve passar para o documento seguinte.

## Ideias por decidir

- **Ver desenhos DWG/DXF/DWF/DWFx (só visualização).** **Decisão de 13/10: adiado 2 a 3 meses
  (por volta de janeiro de 2027), com o CADCraft como base, quando os projetos Craft estiverem
  mais maduros; nessa altura decidimos como juntar os dois.** Pedido de 13/10: rodar em 2D, medir e
  imprimir (sobretudo a Janela) como nos PDF, para substituir o DWG TrueView 2022 e o Design
  Review (usa mais DWF, por vezes DWFx).
  - Abordagem: ao abrir, o desenho é convertido em memória numa página PDF vetorial (com a escala
    do desenho para as medições), e o resto do PeDeeFe (rodar, medir, imprimir com
    Janela/Cartaz) funciona sem código novo. Cada leitor só corre ao abrir esse tipo de ficheiro
    (não pesa no arranque nem nos PDF); pode ficar como opção de compilação (Cargo feature).
  - **DWG e DXF**: biblioteca `acadrust` (Rust puro, lê DWG R13–R2018 e DXF; licença MPL-2.0,
    derivada do ACadSharp, MIT). É a que usa o CADCraft (storytold/cadcraft), projeto da mesma
    equipa do PdfCraft: ver o que dele se pode aproveitar para desenhar entidades (MIT/Apache, a
    confirmar). MPL-2.0 não está na lista de licenças permitidas (deny.toml): é preciso o
    utilizador aceitar juntá-la no fork (obriga só a publicar alterações aos ficheiros da
    própria biblioteca). Sem ela, alternativa: ODA File Converter (gratuito) por trás.
  - **DWFx**: baseado em XPS (norma aberta ECMA-388: ZIP com páginas XML vetoriais); viável.
  - **DWF** (o mais usado pelo utilizador): formato da Autodesk com especificação e toolkit
    publicados, mas sem bibliotecas Rust; leitor próprio, maior esforço. Investigar primeiro com
    ficheiros reais do utilizador (sem os pôr no repositório).
  - Limites: letras SHX trocadas por uma letra parecida; Modelo e Folhas (layouts); camadas como
    no ficheiro; versões DWG anteriores a R13 ou posteriores a 2018 podem não abrir.

## Pendentes de rondas anteriores

- **Impressão Rápida do Outlook:** à espera do teu teste com a test.14 e o PeDeeFe como app
  predefinida para `.pdf`. Hipótese: um botão nas Preferências "Tornar o PeDeeFe a aplicação
  predefinida para PDF", que abre essa página das Definições do Windows.
- **Por enviar para o GitHub:** Ctrl+Tab / Ctrl+Shift+Tab entre separadores e botão do meio
  para fechar um separador (commit bb6cdc8). Só envio quando disseres.
