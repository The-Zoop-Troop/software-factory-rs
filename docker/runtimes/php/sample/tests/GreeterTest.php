<?php
use PHPUnit\Framework\TestCase;
use Sample\Greeter;

final class GreeterTest extends TestCase
{
    public function testGreet(): void
    {
        $this->assertSame("hello rig", Greeter::greet("rig"));
    }
}
