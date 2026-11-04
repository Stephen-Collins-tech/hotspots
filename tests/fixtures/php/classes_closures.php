<?php

namespace App\Service;

use App\Model\User;

interface Greeter
{
    public function greet(string $name): string;
}

abstract class Base
{
    abstract protected function id(): int;

    public function describe(): string
    {
        return 'base#' . $this->id();
    }
}

trait Loggable
{
    public function log(string $message): void
    {
        if ($message !== '') {
            error_log($message);
        }
    }
}

final class UserService extends Base implements Greeter
{
    use Loggable;

    public function __construct(private array $users = [])
    {
    }

    protected function id(): int
    {
        return 1;
    }

    public function greet(string $name): string
    {
        return $name ? "Hello, $name" : 'Hello';
    }

    public function active(): array
    {
        return array_filter($this->users, fn(User $u) => $u->isActive() && !$u->isBanned());
    }

    public function names(): array
    {
        $names = [];
        $collect = function (User $u) use (&$names) {
            if ($u->name) {
                $names[] = $u->name;
            }
        };
        array_map($collect, $this->users);
        return $names;
    }

    public static function make(): static
    {
        return new static();
    }
}

enum Suit: string
{
    case Hearts = 'H';
    case Spades = 'S';

    public function color(): string
    {
        return match ($this) {
            self::Hearts => 'Red',
            self::Spades => 'Black',
        };
    }
}
