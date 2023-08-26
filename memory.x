/* Linker script for the nRF52840 dongle running Nordic's Open Bootloader and SoftDevice S113 7.2.0 */
MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  /* MBR + SoftDevice S113 7.2.0 are 0x1C000 bytes (see SoftDevice release notes) and start at 0x0 */
  /* Bootloader starts at 0xE0000 */
  FLASH : ORIGIN = 0x1C000, LENGTH = 0xE0000 - 0x1C000 
  /* Reserve one 4kB page of flash for constants */
  USER_CONSTANTS (r) : ORIGIN = 0xDF000, LENGTH = 0xE0000 - 0xDF000
  /* MBR + SoftDevice require some amount of RAM. It'll tell us at boot (via logs) what the right
  value is */
  /* Artificially constraining RAM to that of the smallest nRF52 chip */
  RAM : ORIGIN = 0x20000000 + 7952, LENGTH = 24K - 7952
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
/* TODO: investigate further how to access this from application and whether it's actually used */
/* _heap_size = 1024; */
