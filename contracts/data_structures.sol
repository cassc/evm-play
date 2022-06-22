pragma solidity ^0.4.23;
import "./erc20.sol";

contract A{
    uint256 a_0=0;                     // SLOT: 0
    function predict(uint256 val) external returns (uint256){
        a_0 = val + a_0;
        return a_0;
    }
}

contract B {
    enum BEnum { L, T, R, B }
    struct BStruct {
        bytes32 data;
        function(uint256) external returns(uint256) callback;
    }
    
    uint256 public b_0;                // SLOT: 0
    bool public b_1;                   // SLOT: 1, takes 1 byte
    address public b_2;                // SLOT: 1, takes 20 bytes.  (1+20) < 32
    bytes32 public b_3;                // SLOT: 2
    BEnum public b_4;                  // SLOT: 3, dynamic type, because size of function is dynamic
    BStruct[] public b_5;              // SLOT: 4, stores the size of the array in the slot
    mapping(address=>uint256) b_6;     // SLOT: 5, nothing stored in the slot

    constructor() public {
        uint256 init_val = 111111;
        b_0 = init_val;
        b_1 = false;
        b_2 = msg.sender;
        b_3 = bytes32(3200);
        b_4 = BEnum.L;
        b_6[msg.sender] = init_val;
    }
}

contract C is
B,                                     // takes 6 slots
    StandardToken                      // takens 3 slots
{
                                       // all slot numbers below should be added by 8+3 first
    uint256 public c_0;                // SLOT: 0
    bool public c_1;                   // SLOT: 1
    address public c_2;                // SLOT: 1
    bytes32 public c_3;                // SLOT: 2
    BEnum public c_4;                  // SLOT: 4, 5
    BStruct[] public c_5;              // SLOT: 6
    mapping(address=>uint256) c_6;     // SLOT: 7
    mapping(uint256 => uint256[]) c_7; // SLOT: 8
    mapping(uint256 => uint256)[] c_8; // SLOT: 9
    A c_a;                             // SLOT: 10

    string public constant name = "CToken";
    string public constant symbol = "CT"; 
    uint8 public constant decimals = 18;

    uint256 public constant INITIAL_SUPPLY = 10000 * (10 ** uint256(decimals));

    function setVal(uint val) external {
        b_0 = val;
    }

    constructor() public{
        totalSupply_ = INITIAL_SUPPLY;
        balances[msg.sender] = INITIAL_SUPPLY;
        emit Transfer(0x0, msg.sender, INITIAL_SUPPLY);
        
        c_0 = INITIAL_SUPPLY / 2;
        c_1 = false;
        c_2 = msg.sender;
        c_3 = bytes32(3200);
        c_4 = BEnum.L;
        c_6[msg.sender] = INITIAL_SUPPLY / 2;
    }

    function predict() public returns(uint256){
        c_0 = c_5[0].callback(c_0);
        return c_0;
    }

    // This read only function will return out of gas error
    function read() public view returns(uint256){
        while(c_0 > 0){
            // infinite loop
        }
        return c_0;
    }


    function tap(address a_address) public {
        c_a = A(a_address);
        c_5.push(BStruct(c_3, c_a.predict));
    }
}
