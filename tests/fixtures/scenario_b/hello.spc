; hello.spc — Hello World with writeln
; Demonstrates puts (print string) runtime
.data msg 72,101,108,108,111,10,0
.proc main 0
push msg
call puts
halt
.end
.proc puts 1
loada 0
storel 0
lp:
loadl 0
loadb
dup
jz dn
sys 1
loadl 0
push 1
add
storel 0
jmp lp
dn:
drop
ret 1
.end
