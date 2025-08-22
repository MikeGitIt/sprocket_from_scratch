version 1.0

workflow HelloWorld {
  input {
    String name = "World"
  }
  
  call SayHello {
    input: name = name
  }
  
  output {
    String greeting = SayHello.message
  }
}

task SayHello {
  input {
    String name
  }
  
  command <<<
    echo "Hello, ${name}!"
  >>>
  
  output {
    String message = stdout()
  }
}