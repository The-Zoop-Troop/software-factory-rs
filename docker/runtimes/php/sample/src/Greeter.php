<?php
namespace Sample;

final class Greeter
{
    public static function greet(string $name): string
    {
        return "hello $name";
    }
}
