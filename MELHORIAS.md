# PeDeeFe: lista de melhorias

A lista de pedidos de melhorias. A ronda de 14/10 juntou o código original (PdfCraft) com as
nossas alterações e corrigiu os pontos 1 a 4 (ver abaixo); a versão de teste publicada serve
para experimentar a atualização a partir da test.14 (Preferências › Atualizações).

## Por fazer

(Nada por agora.)

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
