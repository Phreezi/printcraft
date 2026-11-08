# PeDeeFe: lista de melhorias

A lista de pedidos de melhorias. A ronda de 7/11 juntou de novo o código original (PdfCraft, que
avançou cerca de 3500 alterações desde 14/10) com as nossas alterações; a versão de teste publicada
instala-se por cima da anterior (Preferências › Atualizações, ou o aviso ao abrir).

## Por fazer

(Nada por agora.)

## Feito na ronda de 7/11 (segunda junção com o PdfCraft original)

O PdfCraft reorganizou quase todo o código (a interface foi dividida em muitos ficheiros e as
traduções passaram para um repositório à parte). As nossas alterações foram transportadas para a
nova organização; onde o PdfCraft passou a fazer o mesmo, ficou o dele:

- **Mantém-se do PeDeeFe:** nome, ícones e "Acerca do"; atualizações a partir das versões de teste
  deste repositório (aviso ao abrir, "Transferir e instalar", o reinício reabre os ficheiros);
  Imprimir com o nosso diálogo (A4/A3, Janela, cartaz, "Imprimir como imagem" predefinido no
  Windows, Enter imprime, cada documento começa em "Todas"); Impressão Rápida do Explorador e do
  Outlook; uma só app por utilizador, com separadores que passam de uma janela para outra e se
  destacam numa janela nova; Página única = uma página por janela, a rolar, com Espaço / Page Down a
  ir para o topo da página seguinte; rolagem automática com o botão do meio também no Windows;
  Editar texto e imagens: apagar vários de uma vez (um só Anular, sem juntar parágrafos vizinhos),
  Ctrl+A, um clique abre o parágrafo, Esc aplica o texto e sai da ferramenta; o painel da direita só
  abre sozinho com as miniaturas de documentos com várias páginas; o painel das ferramentas começa
  fechado; abrir um ficheiro que já está aberto (mesmo nome e tamanho, como um anexo aberto duas
  vezes) mostra o separador dele; linha entre separadores; idioma: Inglês ou Português (Portugal),
  perguntado no primeiro arranque.
- **Passa a ser o do PdfCraft:** clique do meio para fechar um separador e Ctrl+Tab (já os tinha);
  memória da janela (tamanho, posição, maximizada; o PdfCraft corrigiu o mesmo problema de outra
  forma); a correção do 1905.pdf (o PdfCraft corrigiu o mesmo erro no leitor); as palavras da
  assinatura visível no idioma da interface.
- **Traduções:** as tuas 2480 traduções mantêm-se. As cerca de 3000 frases novas do PdfCraft foram
  convertidas do português do Brasil para o de Portugal por regras (Arquivo → Ficheiro, Salvar →
  Guardar, Tela → Ecrã, gerúndios → "a" + infinitivo, etc.). Podem aparecer frases com sabor
  brasileiro: diz-me as que vires e corrijo-as.
- **Novidades do PdfCraft que vêm com esta versão** (entre muitas): Combinar ficheiros em grelha,
  OCR com verificação, mais ferramentas de formulários, comparação de documentos, preferências com
  pesquisa, impressão de etiquetas (no diálogo do PdfCraft, que o nosso substitui), e muitas
  correções de leitura e de desenho de PDF.

## Feito na ronda de 14/10 (junção com o PdfCraft original)

O código original (PdfCraft) foi juntado com as nossas alterações. Onde ambos fizeram o mesmo,
ficou o que serve melhor: no Windows continua predefinida a nossa "Imprimir como imagem" (a que
testaste na Epson), e a impressão nativa do PdfCraft (vetorial, com o botão Propriedades… da
impressora) fica a um clique, desmarcando essa opção; a nossa Página única (uma página por janela, a rolar), o
arrastar do separador para outra janela, a instância única e a rolagem com o botão do meio
mantêm-se; Ctrl+Tab e a reordenação de separadores vêm do PdfCraft.

1. **Abrir no topo da 1.ª página.** Teste novo com 1 e 3 páginas, Ajustar largura, Ajustar
   página, 139% e 400%, contínuo e Página única, e a janela a crescer nos primeiros instantes
   (como ao abrir maximizada): abre sempre no topo da página 1.
2. **1905.pdf.** Causa: ao procurar o fim da imagem do código QR (imagem "em linha"), o leitor
   via acentos mais à frente no texto ("Nº", "ç") e julgava que ainda estava dentro da imagem; a
   palavra "SUJEITO" (tem "EI", o marcador de fim de imagem) fazia a imagem "engolir" o resto da
   página: o QR e todo o texto desapareciam. Corrigido no leitor, com testes sintéticos (o teu
   ficheiro não foi para o repositório). As letras Arial não incorporadas são trocadas por uma
   letra sem serifa embutida.
3. **Aviso de atualizações em cada abertura.** Sempre que o PeDeeFe abre, verifica em segundo
   plano (um pedido pequeno; sem rede, falha em silêncio). Se houver versão nova, abre uma janela
   com "Transferir e instalar", "Mais tarde" e "Não voltar a avisar". "Mais tarde" volta a
   aparecer na abertura seguinte; "Não voltar a avisar" é só para essa versão (volta a avisar
   quando sair outra; desligar de vez continua nas Preferências). A janela espera que outra
   janela (Imprimir, Preferências, palavra-passe…) feche; a impressão rápida (`--print`) não a
   mostra.
4. **Imprimir: cada documento começa em "Todas".** A escolha de páginas (e a Janela) fica
   ligada ao documento; noutro documento volta a Todas. Impressora, papel, cópias e o resto
   continuam lembrados.

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
- Ctrl+Tab / Ctrl+Shift+Tab entre separadores e botão do meio para fechar um separador: enviado
  nesta ronda (Ctrl+Tab passou a ser o do PdfCraft original).
