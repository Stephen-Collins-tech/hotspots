<?php

function describe(int $code): string
{
    switch ($code) {
        case 200:
            return 'ok';
        case 301:
        case 302:
            return 'redirect';
        case 404:
            return 'not found';
        default:
            return 'unknown';
    }
}

function fall_through(int $level): int
{
    $score = 0;
    switch ($level) {
        case 3:
            $score += 3;
        case 2:
            $score += 2;
        case 1:
            $score += 1;
            break;
    }
    return $score;
}

function pick(?string $a, ?string $b): string
{
    $x = $a ?? $b ?? 'default';
    return $x !== '' ? $x : 'empty';
}

function label(int $n): string
{
    return match (true) {
        $n < 0 => 'neg',
        $n === 0 => 'zero',
        default => 'pos',
    };
}

function guard(array $opts): bool
{
    if (isset($opts['a']) && isset($opts['b']) || isset($opts['c'])) {
        return true;
    }
    return false;
}

function alt_syntax(array $rows): void
{
    foreach ($rows as $row):
        if ($row): echo 'x'; elseif ($row === 0): echo 'y'; else: echo 'z'; endif;
    endforeach;
}
