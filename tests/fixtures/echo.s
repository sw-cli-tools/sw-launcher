; tiny COR24 program: write 'A' to UART, then halt.
; Used by step 007 integration tests; small enough to fit at 0x0.
_start:
    la r0, 0x41
    la r1, 0xFF0100
    sb r0, 0(r1)
    .word 0xFFFFFF
