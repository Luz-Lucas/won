; Remove a entrada de início automático que o Sentinela registra em
; HKCU\...\Run (nome do valor = "Sentinela", o que o plugin autostart grava).
; Roda para o usuário que executa o desinstalador; entradas de outros usuários
; do mesmo PC não são alcançadas.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Sentinela"
!macroend
