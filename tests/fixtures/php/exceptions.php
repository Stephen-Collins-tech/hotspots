<?php

function safe_divide(int $a, int $b): int
{
    try {
        return intdiv($a, $b);
    } catch (DivisionByZeroError $e) {
        return 0;
    }
}

function load(string $path): string
{
    try {
        $handle = fopen($path, 'r');
        if (!$handle) {
            throw new RuntimeException('cannot open');
        }
        return stream_get_contents($handle);
    } catch (RuntimeException | InvalidArgumentException $e) {
        log_error($e);
        return '';
    } catch (Throwable $e) {
        throw $e;
    } finally {
        cleanup();
    }
}

function must_be_positive(int $n): int
{
    if ($n <= 0) {
        throw new InvalidArgumentException('positive required');
    }
    return $n;
}
