declare module "rts:JSON" {
  export type i8 = number;
  export type u8 = number;
  export type i16 = number;
  export type u16 = number;
  export type i32 = number;
  export type u32 = number;
  export type i64 = number;
  export type u64 = number;
  export type isize = number;
  export type usize = number;
  export type f32 = number;
  export type f64 = number;
  export type bool = boolean;
  export type str = string;
  /**
   * Serializa um valor para string JSON. Retorna "null" para undefined ou funcoes.
   */
  export function stringify(value: any): string;
  /**
   * Desserializa uma string JSON em um valor. Retorna undefined em caso de erro.
   */
  export function parse(text: string): any;

  const _default: {
    stringify(value: any): string;
    parse(text: string): any;
  };
  export default _default;
}
