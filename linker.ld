SECTIONS
{
    .reloc : {
        _reloc_start = .;
        *(.reloc)
        _reloc_end = .;
    }
}