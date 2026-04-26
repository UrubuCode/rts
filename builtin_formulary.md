apenas analize isso seria uma boa ideia?
a forma imaginada seria a seguinte:

dentro da pasta .rts do node_modules adicione o modules

node_modules/
 - .rts
  - modules/* // o rts vai carregar o modulo dessa area antes do modulo iniciar, 
  - obj/*
  - runtime/*
  tsconfig.json

usaremos o tsconfig.json:

```json
{
    "compilerOptions": {
        "module": "es2022",
        "baseUrl": ".",
        "paths": {
            // package.name
            "node:fs": ["modules/fs/main.ts"],
            // package.alias[0]
            "fs": ["modules/fs/main.ts"]
        },
        "types": [
            // package.types
            "./builtin/rts-types/rts.d.ts"
        ],
        "emitDecoratorMetadata": true,
        "experimentalDecorators": true,
        "composite": true
    },
    
}
```

e o cliente final vai usar:

```json
{
    "references": [
        {
            "path": "./node_modules/.rts/tsconfig.json"
        },
    ]
}
```