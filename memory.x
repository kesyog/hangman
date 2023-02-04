/* Linker script for the nRF52 - WITHOUT SOFT DEVICE */
MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  /* Leave room for Nordic DFU bootloader and MBR */
  /* MBR + SoftDevice S113 7.2.0 are 0x1C000 bytes (see SoftDevice release notes). Bootloader starts at 0xE0000 */
  FLASH : ORIGIN = 0x1C000, LENGTH = 0xE0000 - 0x1C000
  /* MBR + SoftDevice require some TBD amount of RAM, minimum 0x1198 bytes = 4.4kB. Let's just give them 10K for now. */
  RAM : ORIGIN = 0x20000000 + 10K, LENGTH = 256K - 10K
}

/* This is where the call stack will be allocated. */
/* The stack is of the full descending type. */
/* You may want to use this variable to locate the call stack and static
   variables in different memory regions. Below is shown the default value */
/* _stack_start = ORIGIN(RAM) + LENGTH(RAM); */

/* You can use this symbol to customize the location of the .text section */
/* If omitted the .text section will be placed right after the .vector_table
   section */
/* This is required only on microcontrollers that store some configuration right
   after the vector table */
/* _stext = ORIGIN(FLASH) + 0x400; */

/* Size of the heap (in bytes) */
/* _heap_size = 1024; */
