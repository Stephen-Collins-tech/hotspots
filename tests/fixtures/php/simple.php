<?php

function add(int $a, int $b): int
{
    return $a + $b;
}

function classify(int $n): string
{
    if ($n < 0) {
        return 'negative';
    } elseif ($n === 0) {
        return 'zero';
    } else {
        return 'positive';
    }
}

function noop(): void
{
}
